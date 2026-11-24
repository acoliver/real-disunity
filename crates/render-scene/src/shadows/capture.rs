use super::{ShadowError, ShadowedScene, resources};

pub(crate) fn capture_sources(shadowed: bool) -> (&'static str, &'static str) {
    if shadowed {
        (
            resources::SHADOWED_WGSL,
            resources::shadowed_terrain_source(),
        )
    } else {
        (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL)
    }
}
impl ShadowedScene {
    /// Replays against a host's selected frame products: the directional map, the opaque
    /// receiver state and the host's AO and environment bindings.
    pub fn encode_term_capture(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        capture: &crate::compositor::terms::TermCapture,
        scheduler: &crate::compositor::ConfiguredFrameRenderer,
    ) -> Result<(), ShadowError> {
        self.map.retain(encoder)?;
        scheduler.encode_term_capture(
            device,
            encoder,
            capture,
            (&self.gpu, Some((&self.shadow_layout, &self.map.group))),
        );
        Ok(())
    }

    pub(super) fn encode_term_captures(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        captures: &[crate::compositor::terms::TermCapture],
        scheduler: &crate::compositor::FrameRenderer,
    ) -> Result<(), ShadowError> {
        for capture in captures {
            self.map.retain(encoder)?;
            capture.encode_with_frame(
                device,
                encoder,
                &self.gpu,
                crate::compositor::terms::FrameBindings {
                    shadow: Some(crate::compositor::terms::ShadowBinding {
                        layout: &self.shadow_layout,
                        group: &self.map.group,
                    }),
                    frame: Some(scheduler),
                },
            );
        }
        Ok(())
    }
}
