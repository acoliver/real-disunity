//! The renderer's choice of whether the stock passes draw the scene's water.
//!
//! The water the renderer installed is resident whatever the choice: its bodies, its environment
//! and its surface inputs are kept and updated. [`FrameRenderer::stock_water`] is the one query
//! the stock consumers read, so each of them sees the water only while the stock passes draw it.
use super::FrameRenderer;
use crate::draw::WaterDraws;
use crate::water::{Water, WaterFeature, WaterRegistration};
use worldrunner_render_post::FrameError;

impl FrameRenderer {
    /// The water the stock passes draw this frame: the installed water, unless the draws are
    /// suppressed. The water pass and its reactive mask pass, the water receivers, the snapshots
    /// the water reads and the water inputs of the reflections all follow this. Code that holds
    /// other fields of the renderer mutably reads `self.water_draws.drawn(self.water.as_ref())`.
    pub(crate) fn stock_water(&self) -> Option<&Water> {
        self.water_draws.drawn(self.water.as_ref())
    }

    /// Registers the water feature with the choice of draws the renderer was configured with, so
    /// the declaration the chain keeps for it is the one the choice makes before any frame plans.
    pub(super) fn register_water(&mut self, device: &wgpu::Device) -> Result<(), FrameError> {
        self.water_state.borrow_mut().draws = self.water_draws;
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

    /// Hands the registered water feature this frame's water and the choice of draws.
    pub(super) fn record_water_frame(&self, feature: Option<WaterFeature>) {
        let mut state = self.water_state.borrow_mut();
        state.mask_written |= feature.as_ref().is_some_and(WaterFeature::writes_mask);
        state.feature = feature;
        state.draws = self.water_draws;
    }

    /// Zeroes the reactive mask when an earlier frame drew the stock water into it and this frame
    /// draws none, so the temporal resolve does not read a mask of water that is no longer
    /// there. A renderer that never suppresses the water, and every frame after the clear, encode
    /// nothing.
    pub(super) fn clear_unwritten_water_mask(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut state = self.water_state.borrow_mut();
        if state.draws == WaterDraws::Suppressed && std::mem::take(&mut state.mask_written) {
            self.targets().encode_mask_clear(encoder);
        }
    }

    /// Selects the water draws without a configuration, for tests that switch them between
    /// frames of one renderer.
    #[cfg(test)]
    pub(crate) fn force_water_draws(&mut self, draws: WaterDraws) {
        self.water_draws = draws;
        self.water_state.borrow_mut().draws = draws;
    }
}
