use super::{ShadowedScene, resources};

pub(crate) fn capture_sources(shadowed: bool) -> (&'static str, &'static str) {
    if shadowed {
        (resources::SHADOWED_WGSL, resources::SHADOWED_TERRAIN_WGSL)
    } else {
        (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL)
    }
}
impl ShadowedScene {
    /// Reuses the selected frame's directional map and opaque receiver state.
    pub fn encode_term_capture(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        capture: &crate::compositor::terms::TermCapture,
    ) {
        capture.encode_with_frame(
            device,
            encoder,
            &self.gpu,
            (
                Some((&self.shadow_layout, &self.map.group)),
                Some(&self.scheduler),
            ),
        );
    }
    /// Replays against a host's selected frame products instead of the internal scheduler.
    pub fn encode_term_capture_with(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        capture: &crate::compositor::terms::TermCapture,
        scheduler: &crate::compositor::ConfiguredFrameRenderer,
    ) {
        scheduler.encode_term_capture(
            device,
            encoder,
            capture,
            (&self.gpu, Some((&self.shadow_layout, &self.map.group))),
        );
    }
}
