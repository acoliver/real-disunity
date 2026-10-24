use crate::cpu_profile::CpuProfile;
use std::sync::{Arc, mpsc};

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_gpu::surface::{AcquiredFrame, Presenter, SurfaceError, SurfacePolicy};
use worldrunner_render_plan::{RenderScene, SceneTexture};
use worldrunner_render_scene::{
    SceneView, Viewport, device::GpuContext, offscreen::RgbaImage,
    shadows::DirectionalShadowSettings,
};

use crate::{
    LiveFrame, ViewerError,
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
    #[cfg(test)]
    acquisitions: std::collections::VecDeque<wgpu::CurrentSurfaceTexture>,
}

mod resident;
use resident::Resident;

#[derive(Clone, Copy)]
pub(super) enum SceneSource<'a> {
    Static {
        scene: &'a RenderScene,
        shadows: Option<DirectionalShadowSettings>,
    },
    Live,
}

pub(super) struct RenderedFrame {
    pub capture: Option<RgbaImage>,
}

impl Renderer {
    #[cfg(test)]
    pub async fn new(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        scene: &RenderScene,
        shadows: Option<DirectionalShadowSettings>,
    ) -> Result<Self, ViewerError> {
        Self::from_source(display, window, SceneSource::Static { scene, shadows }).await
    }

    pub async fn from_source(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        source: SceneSource<'_>,
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
        CpuProfile::detail("adapter", || format!("{:?}", adapter.get_info()));
        let features = match source {
            SceneSource::Static { scene, .. } => required_features(scene),
            SceneSource::Live => wgpu::Features::empty(),
        };
        let context = GpuContext::from_adapter_with_features(adapter, features).await?;
        Self::attach(instance, surface, window, context, source)
    }

    fn attach(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        window: Arc<Window>,
        context: GpuContext,
        source: SceneSource<'_>,
    ) -> Result<Self, ViewerError> {
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
        let presentation = Presentation::new(context.device(), policy.format, initial.render);
        let resident = Resident::new(&context, source)?;
        let renderer = Self {
            presenter,
            instance,
            window,
            context,
            presentation,
            resident,
            viewport,
            errors,
            #[cfg(test)]
            acquisitions: std::collections::VecDeque::new(),
        };
        renderer.check_errors()?;
        Ok(renderer)
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

    /// Block until every submitted command buffer completes, then surface device errors.
    /// Bounded by the same 30-second deadline as synchronous readback.
    pub fn wait_idle(&self) -> Result<(), ViewerError> {
        self.context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.check_errors()
    }

    pub fn render_viewport(&self) -> Viewport {
        let size = self.viewport.expect("drawable surface");
        Extents::new(
            [size.width, size.height],
            self.context.device().limits().max_texture_dimension_2d,
        )
        .expect("configured surface extent")
        .expect("drawable surface")
        .render
    }

    pub fn prepare_live(&mut self, frame: &LiveFrame) -> Result<(), ViewerError> {
        let _span = CpuProfile::span("viewer_prepare_live");
        CpuProfile::detail("viewer_frame", || {
            format!(
                "viewport={:?} quads={} textures={} texture_bytes={} capture={}",
                frame.view.viewport(),
                frame.quads.len(),
                frame.textures.as_ref().map_or(0, Vec::len),
                frame.textures.as_ref().map_or(0, |textures| textures
                    .iter()
                    .map(|texture| match texture {
                        SceneTexture::Texels { bytes, .. }
                        | SceneTexture::Encoded { bytes, .. } => bytes.len(),
                    })
                    .sum::<usize>()),
                frame.capture.is_some()
            )
        });
        self.resident.prepare_live(&self.context, frame)
    }

    /// Returns whether a frame was actually presented.
    pub fn render(&mut self, view: &SceneView) -> Result<bool, ViewerError> {
        Ok(self.render_capture(view, false)?.is_some())
    }

    pub fn render_capture(
        &mut self,
        view: &SceneView,
        capture: bool,
    ) -> Result<Option<RenderedFrame>, ViewerError> {
        let _span = CpuProfile::span("viewer_render_capture");
        self.check_errors()?;
        if self.viewport.is_none() {
            return Ok(None);
        }
        if self.resident.is_live() {
            self.presentation
                .resize(self.context.device(), view.viewport());
        }
        let span = CpuProfile::span("viewer_acquire");
        let acquired = self.acquire()?;
        drop(span);
        self.render_frame(view, acquired, capture)
    }

    fn acquire(&mut self) -> Result<AcquiredFrame, ViewerError> {
        #[cfg(test)]
        if let Some(acquired) = self.acquisitions.pop_front() {
            return self
                .presenter
                .handle_acquired(self.context.device(), acquired)
                .map_err(surface_error);
        }
        self.presenter
            .acquire(self.context.device())
            .map_err(surface_error)
    }

    #[cfg(test)]
    fn render_acquired(
        &mut self,
        view: &SceneView,
        acquired: wgpu::CurrentSurfaceTexture,
    ) -> Result<bool, ViewerError> {
        let acquired = self
            .presenter
            .handle_acquired(self.context.device(), acquired)
            .map_err(surface_error)?;
        Ok(self.render_frame(view, acquired, false)?.is_some())
    }

    fn render_frame(
        &mut self,
        view: &SceneView,
        acquired: AcquiredFrame,
        capture: bool,
    ) -> Result<Option<RenderedFrame>, ViewerError> {
        let frame = match acquired {
            AcquiredFrame::Frame(frame) => frame,
            AcquiredFrame::Skip | AcquiredFrame::Reconfigure => return Ok(None),
            AcquiredFrame::Recreate => {
                let surface = self.instance.create_surface(self.window.clone())?;
                let capabilities = surface.get_capabilities(self.context.adapter());
                self.presenter
                    .replace_surface(surface, &capabilities, self.context.device())
                    .map_err(surface_error)?;
                return Ok(None);
            }
        };
        let encode = CpuProfile::span("viewer_encode");
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
        if capture {
            self.presentation
                .encode_capture(self.context.device(), &mut encoder)?;
        }
        self.presentation.present(&mut encoder, &output);
        let commands = encoder.finish();
        drop(encode);
        let submit = CpuProfile::span("viewer_submit_present");
        let submission = self.context.queue().submit([commands]);
        self.check_errors()?;
        self.window.pre_present_notify();
        self.context.queue().present(frame);
        drop(submit);
        let capture = if capture {
            let _span = CpuProfile::span("viewer_readback");
            Some(
                self.presentation
                    .read_capture(self.context.device(), submission)?,
            )
        } else {
            None
        };
        Ok(Some(RenderedFrame { capture }))
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

#[cfg(all(test, target_os = "linux"))]
mod tests;
