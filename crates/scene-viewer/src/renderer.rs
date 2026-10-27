use std::sync::{Arc, mpsc};

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_gpu::surface::{Presenter, SurfaceError, SurfacePolicy};

mod capture;
mod execution;
mod live;
mod readiness;
mod retained;
mod target;
mod windowless;
pub(crate) use live::SceneSource;
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
    execution: target::Execution,
    #[cfg(all(test, target_os = "linux"))]
    instance: wgpu::Instance,
    pub window: Option<Arc<Window>>,
    context: GpuContext,
    presentation: Presentation,
    resident: Resident,
    pub viewport: Option<Viewport>,
    errors: mpsc::Receiver<String>,
    clock: crate::clock::Clock,
    pub(crate) frame_time: std::time::Duration,
    live_upload: Option<live::Upload>,
    live_pending: Option<live::Pending>,
    streaming: readiness::Streaming,
}

enum Resident {
    Plain(Box<PlainScene>),
    Shadowed(Box<ShadowedScene>),
}
struct PlainScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    effects: Option<SceneEffects>,
    clear: Option<wgpu::Color>,
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
        let extents = Extents::for_device([size.width, size.height], context.device())?;
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
            initial.surface,
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
            clock: crate::clock::Clock::new(),
            frame_time: std::time::Duration::ZERO,
            live_upload: None,
            live_pending: None,
            streaming: readiness::Streaming::new(shadows, profile),
            execution: target::Execution::Surface(Box::new(execution)),
            #[cfg(all(test, target_os = "linux"))]
            instance: test_instance,
            window: Some(window),
            context,
            presentation,
            resident,
            viewport,
            errors,
        };
        renderer.check_errors().map(|()| renderer)
    }

    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        let extents = Extents::for_device(size, self.context.device())?;
        if let Some(extents) = extents {
            let [width, height] = self
                .presentation
                .scheduler
                .configuration()
                .frame
                .output_extent();
            self.presentation
                .resize(&self.context, Viewport { width, height }, extents.surface)?;
        }
        self.execution.resize(self.context.device(), size)?;
        self.viewport = extents.map(|extents| extents.surface);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
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
        Ok(self.render_admitted(view, update, None, None)?.is_some())
    }

    fn render_admitted(
        &mut self,
        view: &SceneView,
        update: &mut crate::SceneUpdate,
        capture: Option<&mut live::Capture>,
        mut host: Option<&mut crate::RetainedHost>,
    ) -> Result<Option<worldrunner_render_gpu::flight::CompletionTicket>, ViewerError> {
        self.poll()?;
        let resident = &mut self.resident;
        let upload = &mut self.live_upload;

        let Some(ready) = self.execution.acquire(&self.context, || {
            if let Some(upload) = upload.take() {
                resident.upload_live(&self.context, upload)?;
            }
            update(resident.scene_mut()).map_err(ViewerError::from)
        })?
        else {
            return Ok(None);
        };
        let retained_view;
        let view = if let Some(host) = host.as_deref_mut() {
            self.publish_retained(host)?;
            retained_view = host.view()?.expect("admitted retained extent");
            &retained_view
        } else {
            view
        };
        if host
            .as_ref()
            .is_some_and(|host| !host.captures.recipes.is_empty())
        {
            self.presentation.reserve_recipe_storage(&self.context)?;
        }
        self.prepare_frame(view, ready.admission.slot())?;
        let output = ready
            .frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut retained_capture = if let Some(host) = host.as_deref_mut() {
            let pages_ready = self.retained_pages_ready(host, view)?;
            self.streaming.captures.prepare(
                self.context.device(),
                host,
                &ready.frame.texture,
                &self.presentation.scheduler,
                pages_ready,
            )?
        } else {
            None
        };
        let image = retained_capture.as_mut().map(|(_, image)| image);
        let prepared = self.encode_frame(view, capture, &output, image)?;
        let ticket = self
            .execution
            .submit_prepared(ready, prepared, &self.context)?;
        if let Some(host) = host {
            self.streaming.captures.submitted(
                host,
                retained_capture,
                ticket.clone(),
                self.frame_time,
            )?;
        } else {
            self.streaming.captures.advance_frame();
        }
        self.check_errors()?;
        Ok(Some(ticket))
    }

    fn encode_frame(
        &mut self,
        view: &SceneView,
        mut capture: Option<&mut live::Capture>,
        output: &wgpu::TextureView,
        mut retained: Option<&mut capture::Products>,
    ) -> Result<worldrunner_render_gpu::submission::PreparedFrame, ViewerError> {
        if (capture.is_some() || retained.is_some())
            && let Resident::Shadowed(scene) = &mut self.resident
        {
            scene.request_shadow_capture();
        }
        let fitted = self.resident.render_depth_fitted(
            &self.context,
            view,
            &self.presentation.targets,
            self.presentation.scheduler.encoding(),
            (
                Some(self.streaming.captures.next_frame()),
                capture.is_some() || retained.is_some(),
            ),
        )?;
        self.context
            .submission()
            .prepare(self.context.device(), |encoder, _| {
                if !fitted {
                    self.resident.render(
                        &self.context,
                        view,
                        &self.presentation.targets,
                        encoder,
                        self.presentation.scheduler.encoding(),
                    )?;
                }
                self.presentation.present(encoder, output)?;
                if let Some(capture) = &mut capture {
                    capture.encode(encoder, self.presentation.targets.color.texture())?;
                }
                if let Some(retained) = &mut retained {
                    retained.encode(
                        encoder,
                        output.texture(),
                        self.presentation.scheduler.targets().texture(),
                    )?;
                }
                Ok::<_, ViewerError>(())
            })
            .map_err(|error| ViewerError::Gpu(error.to_string()))
    }

    /// Sizes the presentation targets and selects the admitted slot, clear and frame time.
    fn prepare_frame(&mut self, view: &SceneView, slot: usize) -> Result<(), ViewerError> {
        let size = self.viewport.expect("acquired nonzero extent");
        let extents = Extents::for_device([size.width, size.height], self.context.device())?
            .expect("acquired nonzero extent");
        let render_size = if self.resident.clear_color().is_some() {
            view.viewport()
        } else {
            extents.render
        };
        self.presentation
            .resize(&self.context, render_size, extents.surface)?;
        if let Some(clear) = self.resident.clear_color() {
            self.presentation.scheduler.set_clear_color(clear)?;
        }
        self.resident.select_frame_slot(slot);
        self.presentation.scheduler.select_frame_slot(slot);
        self.frame_time = self.clock.time();
        self.presentation
            .scheduler
            .set_frame_time_duration(self.frame_time)?;
        Ok(())
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | (capabilities.usages & wgpu::TextureUsages::COPY_SRC),
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
        Self::from_prepared(
            context,
            worldrunner_render_scene::draw::PreparedScene::new(scene)?,
            shadows,
            profile,
        )
    }

    fn from_prepared(
        context: &GpuContext,
        prepared: worldrunner_render_scene::draw::PreparedScene,
        shadows: Option<DirectionalShadowSettings>,
        profile: crate::FrameProfile,
    ) -> Result<Self, ViewerError> {
        Self::from_prepared_reusing(context, prepared, shadows, profile, None)
    }

    fn from_prepared_reusing(
        context: &GpuContext,
        prepared: worldrunner_render_scene::draw::PreparedScene,
        shadows: Option<DirectionalShadowSettings>,
        profile: crate::FrameProfile,
        previous: Option<&SceneGpu>,
    ) -> Result<Self, ViewerError> {
        let scene = prepared.scene().clone();
        Ok(if let Some(settings) = shadows {
            let mut shadowed = ShadowedScene::with_prepared_target_reusing(
                context,
                prepared,
                settings,
                profile.target().0,
                profile.target().1,
                previous,
            )?;
            shadowed.set_scene_depth_fitting(
                context,
                scene.effects().and_then(|effects| effects.depth_fitting()),
            )?;
            Self::Shadowed(Box::new(shadowed))
        } else {
            if scene
                .effects()
                .and_then(|effects| effects.depth_fitting())
                .is_some()
            {
                return Err(ViewerError::UnsupportedScene(
                    "depth fitting requires directional shadows",
                ));
            }
            let constructor = if scene.effects().and_then(|effects| effects.ssao()).is_some() {
                ScenePipelines::with_ssao
            } else {
                ScenePipelines::with_samples
            };
            let pipelines = constructor(context.device(), profile.target().0, profile.target().1)?;
            let gpu = SceneGpu::upload_prepared_reusing(
                context.device(),
                context.submission(),
                prepared,
                &pipelines,
                previous,
            )?;
            Self::Plain(Box::new(PlainScene {
                gpu,
                pipelines,
                effects: scene.effects().cloned(),
                clear: None,
            }))
        })
    }
    fn render_depth_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        targets: &crate::presentation::Targets,
        mut scheduler: worldrunner_render_scene::compositor::FrameEncoding<'_>,
        frame: (Option<u64>, bool),
    ) -> Result<bool, ViewerError> {
        let Self::Shadowed(scene) = self else {
            return Ok(false);
        };
        if scene.depth_fitting_settings().is_none() {
            return Ok(false);
        }
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
        scheduler.render_depth_fitted(context, view, (&color, &depth), frame, scene)?;
        Ok(true)
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
pub(crate) mod retained_tests;

#[cfg(test)]
mod hdr_tests;

#[cfg(test)]
mod sdsm_tests;
