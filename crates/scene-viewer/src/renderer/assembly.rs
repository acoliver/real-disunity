use super::{
    Presentation, Renderer, Resident, ViewerError, capture, readiness, startup::Resolved, target,
};
use std::sync::{Arc, mpsc};
use winit::window::Window;
use worldrunner_render_scene::{SceneView, Viewport, device::GpuContext};

pub(super) struct RendererParts {
    pub(super) execution: target::Execution,
    #[cfg(all(test, target_os = "linux"))]
    pub(super) instance: wgpu::Instance,
    pub(super) window: Option<Arc<Window>>,
    pub(super) context: GpuContext,
    pub(super) presentation: Presentation,
    pub(super) resident: Resident,
    pub(super) viewport: Option<Viewport>,
    pub(super) errors: mpsc::Receiver<String>,
    pub(super) streaming: readiness::Streaming,
}

impl Renderer {
    pub(super) fn assemble(parts: RendererParts) -> Result<Self, ViewerError> {
        let renderer = Self {
            execution: parts.execution,
            #[cfg(all(test, target_os = "linux"))]
            instance: parts.instance,
            window: parts.window,
            context: parts.context,
            presentation: parts.presentation,
            resident: parts.resident,
            viewport: parts.viewport,
            errors: parts.errors,
            clock: crate::clock::Clock::new(),
            frame_time: std::time::Duration::ZERO,
            game_screen: super::game_screen::Frames::default(),
            streaming: parts.streaming,
        };
        renderer.check_errors()?;
        Ok(renderer)
    }

    pub(super) fn render_depth_fitted(
        &mut self,
        view: &SceneView,
        capture_requested: bool,
        retained: Option<&capture::Products>,
    ) -> Result<bool, ViewerError> {
        self.resident.render_depth_fitted(
            &self.context,
            view,
            &self.presentation.targets,
            self.presentation.scheduler.encoding(),
            (
                Some(self.streaming.captures.next_frame()),
                capture_requested,
                retained.map_or(&[], capture::Products::terms),
            ),
        )
    }
}

pub(super) fn surface_presentation(
    context: &GpuContext,
    policy: worldrunner_render_gpu::surface::SurfacePolicy,
    sizes: [Viewport; 2],
    resolved: &Resolved<'_>,
) -> Result<Presentation, ViewerError> {
    configured_presentation(
        context,
        policy.format,
        sizes,
        resolved.profile,
        resolved.scene.effects(),
        &crate::presentation::PresentationPolicy {
            receiver_gradients: crate::presentation::receiver_reservation(resolved.shadows),
            budget: resolved.startup.presentation_budget,
            capture_pool: crate::CapturePoolPolicy::Fixed(resolved.startup.capture_pool),
            recipes: resolved.startup.recipes,
            game: resolved.startup.game.clone(),
        },
    )
}

pub(super) fn configured_presentation(
    context: &GpuContext,
    format: wgpu::TextureFormat,
    sizes: [Viewport; 2],
    profile: crate::FrameProfile,
    effects: Option<&worldrunner_render_plan::SceneEffects>,
    policy: &crate::presentation::PresentationPolicy,
) -> Result<Presentation, ViewerError> {
    Presentation::new(context, format, sizes, profile, effects, policy)
}
