use std::sync::{Arc, mpsc};

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_gpu::surface::{AcquiredFrame, Presenter, SurfaceError, SurfacePolicy};
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
    presentation::{Extents, Presentation},
};

pub(super) struct Renderer {
    // Surface is dropped before its instance/window; it also owns an Arc<Window>.
    presenter: Presenter<'static>,
    instance: wgpu::Instance,
    pub window: Arc<Window>,
    context: GpuContext,
    presentation: Presentation,
    resident: Resident,
    pub viewport: Option<Viewport>,
    errors: mpsc::Receiver<String>,
}

enum Resident {
    Plain(Box<PlainScene>),
    Shadowed(Box<ShadowedScene>),
}
struct PlainScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    effects: Option<SceneEffects>,
}

impl Renderer {
    pub async fn new(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        scene: &RenderScene,
        shadows: Option<DirectionalShadowSettings>,
        profile: crate::FrameProfile,
    ) -> Result<Self, ViewerError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display)),
        );
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
        let capabilities = surface.get_capabilities(context.adapter());
        let policy = surface_policy(&capabilities, [size.width, size.height])?;
        let presenter = Presenter::new(surface, &capabilities, context.device(), policy)
            .map_err(surface_error)?;
        let presentation = Presentation::with_profile(
            &context,
            policy.format,
            initial.render,
            profile,
            scene.effects(),
        )?;
        let resident = Resident::new(&context, scene, shadows, profile)?;
        let renderer = Self {
            presenter,
            instance,
            window,
            context,
            presentation,
            resident,
            viewport,
            errors,
        };
        renderer.check_errors()?;
        Ok(renderer)
    }

    pub fn scene_mut(&mut self) -> &mut SceneGpu {
        self.resident.scene_mut()
    }

    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        let extents = Extents::new(
            size,
            self.context.device().limits().max_texture_dimension_2d,
        )?;
        self.presenter
            .resize(self.context.device(), size)
            .map_err(surface_error)?;
        self.viewport = extents.map(|extents| extents.surface);
        if let Some(extents) = extents
            && (self.presentation.targets.color.texture().width() != extents.render.width
                || self.presentation.targets.color.texture().height() != extents.render.height)
        {
            self.presentation
                .resize(self.context.device(), extents.render);
        }
        Ok(())
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
        let acquired = self
            .presenter
            .acquire(self.context.device())
            .map_err(surface_error)?;
        self.render_frame(view, acquired)
    }

    #[cfg(all(test, target_os = "linux"))]
    fn render_acquired(
        &mut self,
        view: &SceneView,
        acquired: wgpu::CurrentSurfaceTexture,
    ) -> Result<bool, ViewerError> {
        let acquired = self
            .presenter
            .handle_acquired(self.context.device(), acquired)
            .map_err(surface_error)?;
        self.render_frame(view, acquired)
    }

    fn render_frame(
        &mut self,
        view: &SceneView,
        acquired: AcquiredFrame,
    ) -> Result<bool, ViewerError> {
        let frame = match acquired {
            AcquiredFrame::Frame(frame) => frame,
            AcquiredFrame::Skip | AcquiredFrame::Reconfigure => return Ok(false),
            AcquiredFrame::Recreate => {
                let surface = self.instance.create_surface(self.window.clone())?;
                let capabilities = surface.get_capabilities(self.context.adapter());
                self.presenter
                    .replace_surface(surface, &capabilities, self.context.device())
                    .map_err(surface_error)?;
                return Ok(false);
            }
        };
        let targets = &self.presentation.targets;
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("viewer frame"),
                });
        self.resident.render(
            &self.context,
            view,
            targets,
            &mut encoder,
            &mut self.presentation.scheduler,
        )?;
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

fn surface_error(error: SurfaceError) -> ViewerError {
    match error {
        SurfaceError::Validation => ViewerError::SurfaceValidation,
        other => ViewerError::Gpu(other.to_string()),
    }
}

fn surface_policy(
    capabilities: &wgpu::SurfaceCapabilities,
    size: [u32; 2],
) -> Result<SurfacePolicy, ViewerError> {
    let format = worldrunner_render_gpu::surface::select_format(
        capabilities,
        worldrunner_render_gpu::surface::FormatPreference::Srgb,
    )
    .map_err(|_| ViewerError::SurfaceFormat)?
    .format;
    let alpha_mode = *capabilities
        .alpha_modes
        .first()
        .ok_or(ViewerError::SurfaceFormat)?;
    Ok(SurfacePolicy {
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        size,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode,
    })
}

impl Resident {
    fn scene_mut(&mut self) -> &mut SceneGpu {
        match self {
            Self::Plain(plain) => &mut plain.gpu,
            Self::Shadowed(shadowed) => shadowed.scene_mut(),
        }
    }

    fn new(
        context: &GpuContext,
        scene: &RenderScene,
        shadows: Option<DirectionalShadowSettings>,
        profile: crate::FrameProfile,
    ) -> Result<Self, ViewerError> {
        Ok(if let Some(settings) = shadows {
            Self::Shadowed(Box::new(ShadowedScene::with_target(
                context,
                scene,
                settings,
                profile.target().0,
                profile.target().1,
            )?))
        } else {
            let pipelines = ScenePipelines::with_samples(
                context.device(),
                profile.target().0,
                profile.target().1,
            )?;
            let gpu = SceneGpu::upload(context.device(), context.queue(), scene, &pipelines)?;
            Self::Plain(Box::new(PlainScene {
                gpu,
                pipelines,
                effects: scene.effects().cloned(),
            }))
        })
    }
    fn render(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        targets: &crate::presentation::Targets,
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut worldrunner_render_scene::compositor::FrameRenderer,
    ) -> Result<(), ViewerError> {
        let color = worldrunner_render_post::AttachmentView::new(
            worldrunner_render_post::ResourceRef::new("scene color"),
            targets.color.texture(),
        )
        .map_err(worldrunner_render_scene::shadows::ShadowError::from)?;
        let depth = worldrunner_render_post::AttachmentView::new(
            worldrunner_render_post::ResourceRef::new("scene depth"),
            targets.depth.texture(),
        )
        .map_err(worldrunner_render_scene::shadows::ShadowError::from)?;
        match self {
            Resident::Shadowed(renderer) => {
                renderer.encode_with(context, view, (&color, &depth), encoder, Some(scheduler))?;
            }
            Resident::Plain(plain) => {
                worldrunner_render_scene::compositor::encode_scene(
                    scheduler,
                    context.device(),
                    encoder,
                    (&mut plain.gpu, &plain.pipelines),
                    (&color, &depth),
                    FrameUniform::new(view, plain.effects.as_ref())?,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;

#[cfg(test)]
mod retained_tests;

#[cfg(test)]
mod hdr_tests;
