//! Presentations for tests that do not exercise the policy: default budget, capture pool and
//! game rendering, with receiver gradients reserved for opposing normal bias.

use super::{Presentation, PresentationPolicy};
use crate::{FrameProfile, ViewerError};
use worldrunner_render_scene::{Viewport, device::GpuContext};

pub(crate) fn default_policy() -> PresentationPolicy {
    PresentationPolicy {
        receiver_gradients:
            worldrunner_render_scene::compositor::ReceiverGradientReservation::Opposing,
        budget: crate::PresentationBudget::default().into(),
        capture_pool: crate::CapturePoolPolicy::default(),
        recipes: true,
        game: crate::GameRendering::default(),
    }
}

pub(crate) fn presentation(
    context: &GpuContext,
    format: wgpu::TextureFormat,
    size: Viewport,
) -> Presentation {
    presentation_with_profile(context, format, size, crate::tests::CLAMP, None).unwrap()
}

pub(crate) fn presentation_with_profile(
    context: &GpuContext,
    format: wgpu::TextureFormat,
    size: Viewport,
    profile: FrameProfile,
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> Result<Presentation, ViewerError> {
    Presentation::new(
        context,
        format,
        [size, size],
        profile,
        effects,
        &default_policy(),
    )
}
