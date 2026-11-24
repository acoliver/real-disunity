#[cfg(test)]
mod budget_tests;
mod texture_tiers;

use std::sync::{Arc, mpsc};
use worldrunner_render_scene::shadows::ShadowedTarget;

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_gpu::surface::{Presenter, SurfaceError, SurfacePolicy};

mod assembly;
mod startup;
use startup::Resolved;
mod capture;
mod environment;
mod execution;
mod game_screen;
mod readback;
mod readiness;
mod resource_edit;
mod retained;
mod shared;
pub(crate) use shared::Shared;
mod target;
pub(crate) mod timing;
mod ui_targets;
mod windowless;
use worldrunner_render_plan::{RenderScene, SceneEffects, SceneTexture};
use worldrunner_render_program::offscreen::PendingOffscreen;
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
    streaming: readiness::Streaming,
    game_screen: game_screen::Frames,
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
    #[cfg(all(test, target_os = "linux"))]
    pub async fn new(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        scene: &RenderScene,
        shadows: Option<DirectionalShadowSettings>,
        profile: crate::FrameProfile,
        startup: Startup,
    ) -> Result<Self, ViewerError> {
        Self::surface(display, window, None, |_, _, _| {
            Ok(Resolved {
                scene: std::borrow::Cow::Borrowed(scene),
                shadows,
                profile,
                startup,
            })
        })
        .await
    }

    /// As [`Self::new`] with no shadows and the clamped HDR test profile, on a device made before
    /// the window.
    #[cfg(all(test, target_os = "linux"))]
    pub async fn new_shared(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        scene: &RenderScene,
        shared: Shared,
    ) -> Result<Self, ViewerError> {
        Self::surface(display, window, Some(shared), |_, _, _| {
            Ok(Resolved {
                scene: std::borrow::Cow::Borrowed(scene),
                shadows: None,
                profile: crate::tests::CLAMP,
                startup: 2.into(),
            })
        })
        .await
    }

    /// A renderer for the window of `host`, whose initial scene and options decide the startup.
    pub async fn from_host(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        host: &mut crate::RetainedHost,
        shared: Option<Shared>,
    ) -> Result<Self, ViewerError> {
        let monitor = crate::window_size::display_info(&window);
        Self::surface(
            display,
            window,
            shared,
            |adapter, surface_extent, census| {
                startup::resolve(host, adapter, surface_extent, monitor, census)
            },
        )
        .await
    }

    async fn surface<'a>(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        shared: Option<Shared>,
        resolve: impl FnOnce(
            &wgpu::Adapter,
            [u32; 2],
            Option<&[wgpu::Backend]>,
        ) -> Result<Resolved<'a>, ViewerError>,
    ) -> Result<Self, ViewerError> {
        let (instance, surface, context, policy, resolved) =
            shared::surface_context(display, window.clone(), shared, resolve).await?;
        let errors = device_errors(context.device(), context.submission());
        let extents = Extents::for_device(policy.size, context.device())?;
        let initial = extents.unwrap_or_else(Extents::placeholder);
        let viewport = extents.map(|extents| extents.surface);
        let mut presentation = assembly::surface_presentation(
            &context,
            policy,
            [initial.render, initial.surface],
            &resolved,
        )?;
        let Resolved {
            scene,
            shadows,
            profile,
            startup,
        } = resolved;
        let scene = scene.as_ref();
        let presenter = Presenter::new(surface, context.adapter(), context.device(), policy)
            .map_err(surface_error)?;
        let mut streaming = readiness::Streaming::new(shadows, profile);
        streaming.tiers.policy = startup.texture_tiers;
        let resident = environment::new_resident(
            &context,
            scene,
            shadows,
            profile,
            &mut presentation,
            &mut streaming.tiers,
        )?;
        #[cfg(all(test, target_os = "linux"))]
        let test_instance = instance.clone();
        let execution = execution::SurfaceExecution::new(
            presenter,
            instance,
            window.clone(),
            startup.frames_in_flight,
        )?;
        Self::assemble(assembly::RendererParts {
            execution: target::Execution::Surface(Box::new(execution)),
            #[cfg(all(test, target_os = "linux"))]
            instance: test_instance,
            window: Some(window),
            context,
            presentation,
            resident,
            viewport,
            errors,
            streaming,
        })
    }

    pub(crate) fn surface_extent(&self) -> [u32; 2] {
        let target::Execution::Surface(surface) = &self.execution else {
            panic!("windowed renderer");
        };
        let configuration = surface.presenter.configuration();
        [configuration.width, configuration.height]
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

    /// Whether a frame would be admitted now. Completed submissions retire only when the device
    /// is polled, so call this after [`Self::poll`].
    pub(crate) fn frame_slot_free(&self) -> bool {
        self.execution.flight().has_free_slot()
    }

    /// Draws `view` with no host. Admission and completion progress never wait for GPU work on the
    /// event thread. Only the Linux window tests (`mod tests`) draw without a host.
    #[cfg(all(test, target_os = "linux"))]
    pub fn render(&mut self, view: &SceneView) -> Result<bool, ViewerError> {
        let Some(ready) = self.acquire_render_frame()? else {
            return Ok(false);
        };
        self.draw_admitted(ready, view, None)?;
        Ok(true)
    }

    /// Draws the host's view when a frame is admitted. The view is resolved once, after admission
    /// and publication, so a frame that is not admitted resolves no camera.
    fn render_admitted(
        &mut self,
        host: &mut crate::RetainedHost,
    ) -> Result<Option<worldrunner_render_gpu::flight::CompletionTicket>, ViewerError> {
        if host.offscreen.is_some() && !self.admit_retained_offscreen(host)? {
            return Ok(None);
        }
        let Some(ready) = self.acquire_render_frame()? else {
            return Ok(None);
        };
        if host.offscreen.is_none() {
            self.publish_retained(host)?;
        }
        self.apply_retained_shadow_policy(host)?;
        let view = host.view()?.expect("admitted retained extent");
        self.draw_admitted(ready, &view, Some(host)).map(Some)
    }

    fn draw_admitted(
        &mut self,
        ready: execution::ReadyFrame,
        view: &SceneView,
        mut host: Option<&mut crate::RetainedHost>,
    ) -> Result<worldrunner_render_gpu::flight::CompletionTicket, ViewerError> {
        if let Some(host) = host.as_deref() {
            self.reject_offscreen_depth_fitting(host)?;
        }
        self.prepare_frame(view, ready.admission.slot())?;
        if let Some(host) = host.as_deref() {
            self.prepare_camera_history(host);
        }
        let output = ready
            .frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // The offer comes first: capture preparation refuses what a screen frame cannot supply,
        // and it does so before the frame takes the host's queued operations.
        if let Some(host) = host.as_deref() {
            self.streaming.ui.offer(
                &self.context,
                (&mut self.resident, &mut self.presentation),
                &host.ui_targets,
            )?;
        }
        self.game_screen
            .offer(host.as_deref().and_then(|host| host.game_screen.as_ref()));
        self.bind_ui_layers(host.as_deref().and_then(|host| host.ui_layers.as_ref()))?;
        let capture::HostCapture {
            request: mut retained_capture,
            misses,
        } = self.prepare_host_capture(host.as_deref_mut(), view, &ready.frame.texture)?;
        let image = retained_capture.as_mut().map(|(_, image)| image);
        let offscreen = host.as_deref_mut().and_then(|host| host.offscreen.take());
        let prepared = self.encode_frame(view, offscreen, &output, image, misses)?;
        let ticket = self
            .execution
            .submit_prepared(ready, prepared, &self.context)?;
        self.game_screen.submitted();
        retained::finish_admitted_frame(self, host, view, retained_capture, ticket.clone())?;
        Ok(ticket)
    }

    /// Points the overlay's runs of UI draws at the texture the host offers for them.
    fn bind_ui_layers(&mut self, layers: Option<&wgpu::Texture>) -> Result<(), ViewerError> {
        self.resident
            .scene_mut()
            .bind_ui_layers(self.context.device(), self.context.submission(), layers)
            .map_err(|error| ViewerError::Gpu(error.to_string()))
    }

    fn apply_retained_shadow_policy(
        &mut self,
        host: &crate::RetainedHost,
    ) -> Result<(), ViewerError> {
        let policy = self
            .streaming
            .shadow_policy
            .unwrap_or(match host.options.shadows {
                Some(_) => crate::CameraShadowPolicy::Enabled,
                None => crate::CameraShadowPolicy::Disabled,
            });
        self.resident.apply_shadow_policy(policy)
    }

    fn acquire_render_frame(&mut self) -> Result<Option<execution::ReadyFrame>, ViewerError> {
        self.poll()?;
        self.execution.acquire(&self.context)
    }

    fn encode_frame(
        &mut self,
        view: &SceneView,
        mut offscreen: Option<PendingOffscreen>,
        output: &wgpu::TextureView,
        retained: Option<&mut capture::Products>,
        misses: game_screen::ViewMisses,
    ) -> Result<worldrunner_render_gpu::submission::PreparedFrame, ViewerError> {
        let fitted = self.fit_stock_scene(view, retained.as_deref())?;
        self.context
            .submission()
            .prepare(self.context.device(), |encoder, _| {
                if !fitted {
                    self.presentation
                        .scheduler
                        .begin_timing(self.streaming.captures.next_frame())
                        .map_err(|error| ViewerError::Gpu(error.to_string()))?;
                    self.game_screen.render(
                        &mut self.resident,
                        (&self.context, view, &self.presentation.targets),
                        encoder,
                        self.presentation.scheduler.encoding(),
                        game_screen::ScreenWork {
                            operations: offscreen.take(),
                            misses,
                        },
                    )?;
                }
                self.presentation.present(encoder, output)?;
                capture::Encoding {
                    device: self.context.device(),
                    resident: &self.resident,
                    presentation: &self.presentation,
                }
                .encode(encoder, output, retained, fitted)
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
        self.presentation.scheduler.set_frame_time(self.frame_time);
        Ok(())
    }
}

fn required_features(scene: &RenderScene) -> wgpu::Features {
    if scene
        .resources()
        .textures()
        .iter()
        .any(|texture| matches!(texture.payload(), SceneTexture::Encoded { .. })
            || matches!(texture.payload(), SceneTexture::SpecularCube(cube) if cube.format() == worldrunner_render_plan::SceneSpecularCubeFormat::Bc6hRgbUfloat))
    {
        wgpu::Features::TEXTURE_COMPRESSION_BC
    } else {
        wgpu::Features::empty()
    }
}

fn device_errors(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
) -> mpsc::Receiver<String> {
    let (send, errors) = mpsc::channel();
    let lost_send = send.clone();
    let report_loss = authority.device_loss_reporter();
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        let _ = send.send(error.to_string());
    }));
    device.set_device_lost_callback(move |reason, message| {
        report_loss(format!("{reason:?}: {message}"));
        let _ = lost_send.send(format!("device lost ({reason:?}): {message}"));
    });
    errors
}

fn surface_error(error: SurfaceError) -> ViewerError {
    match error {
        SurfaceError::UnsupportedPresentMode(mode) => ViewerError::UnsupportedPresentMode(mode),
        SurfaceError::Validation => ViewerError::SurfaceValidation,
        SurfaceError::Extent { size, limit } => ViewerError::SurfaceExtent {
            size,
            device_limit: limit,
        },
        other => ViewerError::Gpu(other.to_string()),
    }
}

pub(crate) fn surface_policy(
    capabilities: &wgpu::SurfaceCapabilities,
    size: [u32; 2],
    startup: &Startup,
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
    let policy = SurfacePolicy {
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | (capabilities.usages & wgpu::TextureUsages::COPY_SRC),
        size,
        present_mode: startup.present_mode,
        desired_maximum_frame_latency: startup.desired_maximum_frame_latency,
        alpha_mode,
    };
    worldrunner_render_gpu::surface::configuration(capabilities, policy).map_err(surface_error)?;
    Ok(policy)
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

    fn depth_fitting_enabled(&self) -> bool {
        match self {
            Self::Plain(_) => false,
            Self::Shadowed(scene) => scene.depth_fitting_settings().is_some(),
        }
    }

    fn apply_shadow_policy(
        &mut self,
        policy: crate::CameraShadowPolicy,
    ) -> Result<(), ViewerError> {
        match (self, policy) {
            (Self::Shadowed(scene), crate::CameraShadowPolicy::Enabled) => {
                scene.set_shadows_enabled(true);
            }
            (Self::Shadowed(scene), crate::CameraShadowPolicy::Disabled) => {
                scene.set_shadows_enabled(false);
            }
            (Self::Plain(_), crate::CameraShadowPolicy::Enabled) => {
                return Err(ViewerError::UnsupportedScene(
                    "directional shadows require a shadowed retained scene",
                ));
            }
            (Self::Plain(_), crate::CameraShadowPolicy::Disabled) => {}
        }
        Ok(())
    }

    #[cfg(test)]
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
            let mut shadowed = ShadowedScene::new(
                context,
                prepared,
                settings,
                ShadowedTarget {
                    previous,
                    ..ShadowedTarget::new(profile.target().0, profile.target().1)
                },
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
                ScenePipelines::new
            };
            let pipelines = constructor(context.device(), profile.target().0, profile.target().1)?;
            let gpu = SceneGpu::upload(
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
        frame: (
            Option<u64>,
            bool,
            &[worldrunner_render_scene::compositor::terms::TermCapture],
        ),
    ) -> Result<bool, ViewerError> {
        let Self::Shadowed(scene) = self else {
            return Ok(false);
        };
        if scene.depth_fitting_settings().is_none() {
            return Ok(false);
        }
        let (color, depth) = targets
            .scene_attachments()
            .map_err(worldrunner_render_scene::shadows::ShadowError::from)?;
        scheduler.render_depth_fitted(context, view, (color, depth), frame, scene)?;
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
        let (color, depth) = targets
            .scene_attachments()
            .map_err(worldrunner_render_scene::shadows::ShadowError::from)?;
        match self {
            Resident::Shadowed(renderer) => {
                scheduler.encode_shadowed(context, view, (color, depth), encoder, renderer)?;
            }
            Resident::Plain(plain) => {
                scheduler.encode_scene(
                    context.device(),
                    encoder,
                    (&mut plain.gpu, &plain.pipelines),
                    worldrunner_render_scene::compositor::ColorDepth { color, depth },
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
mod camera_cut_tests;

#[cfg(test)]
mod backend_census_tests;

#[cfg(test)]
pub(crate) mod sdsm_tests;

#[derive(Clone, Debug)]
pub(crate) struct Startup {
    pub(crate) presentation_budget: crate::PresentationBudgetPolicy,
    pub(crate) capture_pool: u64,
    pub(crate) recipes: bool,
    pub(crate) texture_tiers: Option<worldrunner_render_scene::texture::TextureTierPolicy>,
    pub(crate) game: crate::GameRendering,
    frames_in_flight: usize,
    gpu_timing: bool,
    present_mode: wgpu::PresentMode,
    desired_maximum_frame_latency: u32,
}
impl From<&crate::ViewerOptions> for Startup {
    fn from(options: &crate::ViewerOptions) -> Self {
        Self {
            presentation_budget: options.presentation_budget,
            capture_pool: 256 * 1024 * 1024,
            recipes: false,
            texture_tiers: options.texture_tiers,
            game: options.game.clone(),
            frames_in_flight: options.frames_in_flight,
            gpu_timing: options.gpu_timing,
            present_mode: options.present_mode,
            desired_maximum_frame_latency: options.desired_maximum_frame_latency,
        }
    }
}
impl From<usize> for Startup {
    fn from(frames_in_flight: usize) -> Self {
        Self {
            presentation_budget: crate::PresentationBudget::default().into(),
            capture_pool: 256 * 1024 * 1024,
            recipes: false,
            texture_tiers: None,
            game: crate::GameRendering::default(),
            frames_in_flight,
            gpu_timing: false,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
        }
    }
}

#[cfg(test)]
mod adapter_tests;

#[cfg(test)]
mod adapter_frame_tests;

#[cfg(test)]
mod adapter_content_tests;
