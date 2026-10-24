use std::sync::{Arc, mpsc};

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_gpu::surface::{Presenter, SurfaceError, SurfacePolicy};

mod execution;
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
    execution: execution::SurfaceExecution,
    #[cfg(all(test, target_os = "linux"))]
    instance: wgpu::Instance,
    pub window: Arc<Window>,
    context: GpuContext,
    presentation: Presentation,
    resident: Resident,
    pub viewport: Option<Viewport>,
    errors: mpsc::Receiver<String>,
    animation_start: std::time::Instant,
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
        frames_in_flight: usize,
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
        let errors = device_errors(context.device(), context.submission().clone());
        let size = window.inner_size();
        let extents = Extents::new(
            [size.width, size.height],
            context.device().limits().max_texture_dimension_2d,
        )?;
        let initial = extents.unwrap_or_else(Extents::placeholder);
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
        #[cfg(all(test, target_os = "linux"))]
        let test_instance = instance.clone();
        let execution = execution::SurfaceExecution::new(
            presenter,
            instance,
            window.clone(),
            frames_in_flight,
        )?;
        let renderer = Self {
            animation_start: std::time::Instant::now(),
            execution,
            #[cfg(all(test, target_os = "linux"))]
            instance: test_instance,
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

    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        let extents = Extents::new(
            size,
            self.context.device().limits().max_texture_dimension_2d,
        )?;
        self.execution.resize(self.context.device(), size)?;
        self.viewport = extents.map(|extents| extents.surface);
        self.window.request_redraw();
        Ok(())
    }

    pub fn check_errors(&self) -> Result<(), ViewerError> {
        if let Ok(error) = self.errors.try_recv() {
            return Err(ViewerError::Gpu(error));
        }
        Ok(())
    }

    pub fn poll(&self) -> Result<(), ViewerError> {
        self.context
            .device()
            .poll(wgpu::PollType::Poll)
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.check_errors()
    }

    /// Admission and completion progress never wait for GPU work on the event thread.
    pub fn render(
        &mut self,
        view: &SceneView,
        update: &mut crate::SceneUpdate,
    ) -> Result<bool, ViewerError> {
        self.poll()?;
        let resident = &mut self.resident;
        let Some(ready) = self.execution.acquire(&self.context, || {
            update(resident.scene_mut()).map_err(ViewerError::from)
        })?
        else {
            return Ok(false);
        };
        let size = self.viewport.expect("acquired nonzero extent");
        let extents = Extents::new(
            [size.width, size.height],
            self.context.device().limits().max_texture_dimension_2d,
        )?
        .expect("acquired nonzero extent");
        self.presentation.resize(&self.context, extents.render)?;
        self.resident.select_frame_slot(ready.admission.slot());
        self.presentation
            .scheduler
            .select_frame_slot(ready.admission.slot());
        self.presentation
            .scheduler
            .set_frame_time(self.animation_start.elapsed().as_secs_f32())?;
        let output = ready
            .frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let prepared = self
            .context
            .submission()
            .prepare(self.context.device(), |encoder, _| {
                self.resident.render(
                    &self.context,
                    view,
                    &self.presentation.targets,
                    encoder,
                    self.presentation.scheduler.encoding(),
                )?;
                self.presentation.present(encoder, &output)
            })
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.execution
            .submit_prepared(ready, prepared, &self.context)?;
        self.check_errors()?;
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

fn device_errors(
    device: &wgpu::Device,
    authority: worldrunner_render_gpu::submission::SubmissionAuthority,
) -> mpsc::Receiver<String> {
    let (send, errors) = mpsc::channel();
    let lost_send = send.clone();
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        let _ = send.send(error.to_string());
    }));
    device.set_device_lost_callback(move |reason, message| {
        authority.report_device_loss(format!("{reason:?}: {message}"));
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
    fn select_frame_slot(&mut self, slot: usize) {
        match self {
            Self::Plain(plain) => plain.gpu.select_frame_slot(slot),
            Self::Shadowed(shadowed) => shadowed.select_frame_slot(slot),
        }
    }
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
            let constructor = if scene.effects().and_then(|effects| effects.ssao()).is_some() {
                ScenePipelines::with_ssao
            } else {
                ScenePipelines::with_samples
            };
            let pipelines = constructor(context.device(), profile.target().0, profile.target().1)?;
            let gpu = SceneGpu::upload(context.device(), context.submission(), scene, &pipelines)?;
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
        mut scheduler: worldrunner_render_scene::compositor::FrameEncoding<'_>,
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
                scheduler.encode_shadowed(context, view, (&color, &depth), encoder, renderer)?;
            }
            Resident::Plain(plain) => {
                scheduler.encode_scene(
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
