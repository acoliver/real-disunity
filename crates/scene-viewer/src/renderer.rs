use std::sync::{Arc, mpsc};

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_plan::{RenderScene, SceneEffects, SceneTexture};
use worldrunner_render_scene::{
    SceneView, Viewport,
    device::GpuContext,
    draw::SceneGpu,
    frame::FrameUniform,
    renderer::ScenePipelines,
    shadows::{DirectionalShadowSettings, ShadowedScene},
};

use crate::{
    ViewerError,
    presentation::{COLOR, Extents, Presentation},
};

pub(super) struct Renderer {
    // Surface is dropped before its instance/window; it also owns an Arc<Window>.
    surface: wgpu::Surface<'static>,
    instance: wgpu::Instance,
    pub window: Arc<Window>,
    context: GpuContext,
    config: wgpu::SurfaceConfiguration,
    presentation: Presentation,
    resident: Resident,
    pub viewport: Option<Viewport>,
    errors: mpsc::Receiver<String>,
    #[cfg(test)]
    configurations: usize,
}

enum Resident {
    Plain(Box<PlainScene>),
    Shadowed(Box<ShadowedScene>),
}
struct PlainScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    effects: SceneEffects,
}

impl Renderer {
    pub async fn new(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        scene: &RenderScene,
        shadows: Option<DirectionalShadowSettings>,
    ) -> Result<Self, ViewerError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let features = required_features(scene);
        let context = GpuContext::from_adapter_with_features(adapter, features).await?;
        let errors = device_errors(context.device());
        let size = window.inner_size();
        let extents = Extents::new(
            [size.width, size.height],
            context.device().limits().max_texture_dimension_2d,
        )?;
        let initial = extents.unwrap_or(Extents {
            surface: Viewport {
                width: 1,
                height: 1,
            },
            render: Viewport {
                width: 1,
                height: 1,
            },
        });
        let viewport = extents.map(|extents| extents.surface);
        let config = surface_config(&surface, &context, initial.surface)?;
        if viewport.is_some() {
            surface.configure(context.device(), &config);
        }
        let presentation = Presentation::new(context.device(), config.format, initial.render);
        let resident = Resident::new(&context, scene, shadows)?;
        let renderer = Self {
            surface,
            instance,
            window,
            context,
            config,
            presentation,
            resident,
            viewport,
            errors,
            #[cfg(test)]
            configurations: usize::from(viewport.is_some()),
        };
        renderer.check_errors()?;
        Ok(renderer)
    }

    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        let extents = Extents::new(
            size,
            self.context.device().limits().max_texture_dimension_2d,
        )?;
        self.viewport = extents.map(|extents| extents.surface);
        if let Some(extents) = extents {
            if self.presentation.targets.color.texture().width() != extents.render.width
                || self.presentation.targets.color.texture().height() != extents.render.height
            {
                self.presentation
                    .resize(self.context.device(), extents.render);
            }
            self.config.width = extents.surface.width;
            self.config.height = extents.surface.height;
            self.configure();
        }
        Ok(())
    }

    fn configure(&mut self) {
        self.surface.configure(self.context.device(), &self.config);
        #[cfg(test)]
        {
            self.configurations += 1;
        }
    }

    pub fn check_errors(&self) -> Result<(), ViewerError> {
        if let Ok(error) = self.errors.try_recv() {
            return Err(ViewerError::Gpu(error));
        }
        Ok(())
    }

    /// Returns whether a frame was actually presented (not merely requested).
    pub fn render(&mut self, view: &SceneView) -> Result<bool, ViewerError> {
        self.check_errors()?;
        if self.viewport.is_none() {
            return Ok(false);
        }
        self.render_acquired(view, self.surface.get_current_texture())
    }

    fn render_acquired(
        &mut self,
        view: &SceneView,
        acquired: wgpu::CurrentSurfaceTexture,
    ) -> Result<bool, ViewerError> {
        let frame = match acquired {
            // Suboptimal is usable. Reconfiguration is left to resize/outdated:
            // dropping it can starve presentation forever on some platforms.
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure();
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(self.window.clone())?;
                self.configure();
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => return Err(ViewerError::SurfaceValidation),
        };
        let targets = &self.presentation.targets;
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("viewer frame"),
                });
        self.resident
            .render(&self.context, view, targets, &mut encoder)?;
        let output = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.presentation.present(&mut encoder, &output);
        self.context.queue().submit([encoder.finish()]);
        self.check_errors()?;
        self.window.pre_present_notify();
        self.context.queue().present(frame);
        Ok(true)
    }
}

fn required_features(scene: &RenderScene) -> wgpu::Features {
    if scene
        .resources()
        .textures()
        .iter()
        .any(|texture| matches!(texture, SceneTexture::Encoded { .. }))
    {
        wgpu::Features::TEXTURE_COMPRESSION_BC
    } else {
        wgpu::Features::empty()
    }
}

fn device_errors(device: &wgpu::Device) -> mpsc::Receiver<String> {
    let (send, errors) = mpsc::channel();
    let lost_send = send.clone();
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        let _ = send.send(error.to_string());
    }));
    device.set_device_lost_callback(move |reason, message| {
        let _ = lost_send.send(format!("device lost ({reason:?}): {message}"));
    });
    errors
}

fn surface_config(
    surface: &wgpu::Surface<'_>,
    context: &GpuContext,
    size: Viewport,
) -> Result<wgpu::SurfaceConfiguration, ViewerError> {
    let mut config = surface
        .get_default_config(context.adapter(), size.width, size.height)
        .ok_or(ViewerError::SurfaceFormat)?;
    config.format = surface
        .get_capabilities(context.adapter())
        .formats
        .into_iter()
        .find(|format| {
            matches!(
                format,
                wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
            )
        })
        .ok_or(ViewerError::SurfaceFormat)?;
    config.present_mode = wgpu::PresentMode::Fifo;
    config.desired_maximum_frame_latency = 2;
    Ok(config)
}

impl Resident {
    fn new(
        context: &GpuContext,
        scene: &RenderScene,
        shadows: Option<DirectionalShadowSettings>,
    ) -> Result<Self, ViewerError> {
        Ok(if let Some(settings) = shadows {
            Self::Shadowed(Box::new(ShadowedScene::new(context, scene, settings)?))
        } else {
            let pipelines = ScenePipelines::new(context.device(), COLOR);
            let gpu = SceneGpu::upload(context.device(), context.queue(), scene, &pipelines)?;
            Self::Plain(Box::new(PlainScene {
                gpu,
                pipelines,
                effects: scene.effects().clone(),
            }))
        })
    }
    fn render(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        targets: &crate::presentation::Targets,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), ViewerError> {
        match self {
            Resident::Shadowed(renderer) => {
                renderer.render(context, view, &targets.color, &targets.depth)?;
            }
            Resident::Plain(plain) => {
                let PlainScene {
                    gpu,
                    pipelines,
                    effects,
                } = plain.as_ref();
                gpu.set_frame(context.queue(), FrameUniform::new(view, effects)?);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("viewer scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.color,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &targets.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                gpu.draw(&mut pass, pipelines);
            }
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
