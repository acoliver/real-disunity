//! The stock passes' view of the scene's water, which a game feature may draw instead.
//!
//! The water the renderer installed is resident whatever a game declares: its bodies, its
//! environment and its surface inputs are kept and updated. [`FrameRenderer::stock_water`] is the
//! one query the stock consumers read, so each of them sees the water only while the stock passes
//! draw it.
use super::FrameRenderer;
use crate::water::{Water, WaterRegistration};
use worldrunner_render_post::FrameError;

impl FrameRenderer {
    /// The water the stock passes draw this frame: the installed water, unless a game feature
    /// declared that it draws the water. The water pass, the water receivers, the snapshots
    /// the water reads and the water inputs of the reflections all follow this. Code that holds
    /// other fields of the renderer mutably reads
    /// `self.game.replaces.stock_water(self.water.as_ref())`, which is the same rule.
    pub(crate) fn stock_water(&self) -> Option<&Water> {
        self.game.replaces.stock_water(self.water.as_ref())
    }

    /// Registers the stock water feature, unless a game feature draws the water: then the chain
    /// carries no water feature and the frame validates as it does without water.
    pub(super) fn register_water(&mut self, device: &wgpu::Device) -> Result<(), FrameError> {
        if self.game.replaces.water {
            return Ok(());
        }
        let samples = self.targets().config().samples;
        self.chain
            .register(
                device,
                Box::new(WaterRegistration {
                    state: self.water_state.clone(),
                    samples,
                }),
            )
            .map(|_| ())
    }

    /// Whether a frame of this renderer reads the opaque snapshot banks: the stock water draws,
    /// reflections, depth-tested overlay geometry, or a feature of the game's inventory. Water
    /// that the stock passes do not draw reads nothing.
    pub(super) fn snapshot_readers(&self) -> bool {
        self.stock_water().is_some()
            || self.ssr.is_some()
            || self.overlay.needs_depth()
            || self.game.snapshot_reads.any()
    }

    /// Whether the allocated snapshot banks are written: a reader above, or a caller feature that
    /// asked for a snapshot in a frame this renderer has already planned.
    pub(super) fn snapshots_written(&self) -> bool {
        self.snapshot_readers() || self.snapshot_ready.borrow().iter().any(|ready| ready.any())
    }

    /// Submitted per-sample water receivers: view normal and water flag (alpha),
    /// dielectric F0 and environment roughness, and replaceable environment specular.
    /// Sample-frequency normal/coverage, material (pixel offsets RG, scalar F0 B,
    /// environment-coordinate roughness A), and specular (camera transmittance A).
    pub fn water_receivers(
        &self,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView, wgpu::TextureView)> {
        self.stock_water()
            .filter(|_| self.snapshot_ready.borrow()[self.frame_slot].final_depth)
            .map(|water| {
                let [normal, material, specular] = &water.receivers[self.frame_slot];
                (
                    normal.create_view(&wgpu::TextureViewDescriptor::default()),
                    material.create_view(&wgpu::TextureViewDescriptor::default()),
                    specular.create_view(&wgpu::TextureViewDescriptor::default()),
                )
            })
    }
}
