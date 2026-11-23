//! The frozen-plan entry of the frame renderer.
#[cfg(test)]
use super::plan_trace;
use super::{FrameOutput, FrameRenderer};
use worldrunner_render_post::{BorrowedFeature, FrameError, FramePlan, PostChain};

impl FrameRenderer {
    /// Encodes a complete frame plan exactly as given: the entries in the order listed, the
    /// views, the requested effects and the operations groups at the positions they name. Nothing
    /// is added, removed or sorted, and none of the stock helpers (screen-space effects,
    /// reflections, snapshots, retained-feature planning, the forward order) runs;
    /// those belong to the stock builder, which the caller replaces by authoring the plan.
    /// [`Self::encode_frame`] is this entry fed the stock builder's plan.
    ///
    /// The features are the ones the plan's entries name, with the declarations they were
    /// planned under. Retained features registered on the renderer take part with the
    /// declarations they last planned.
    ///
    /// # Errors
    /// The errors of validating the plan, which are returned before any feature prepares, and
    /// of preparing it.
    pub fn encode_frozen(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &FramePlan<'_>,
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        self.publish_snapshots(encoder, crate::water::planning::SnapshotRequest::default());
        self.remember_taa_block(None);
        encode_frozen_plan(&mut self.chain, device, encoder, plan, features)?;
        self.record_game_work(encoder)
    }
}

/// Encodes a complete plan exactly as given. Everything that adds, removes or orders entries has
/// run by now: the stock builder for a stock frame, the caller for a frozen one.
pub(super) fn encode_frozen_plan(
    chain: &mut PostChain<FrameOutput>,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    plan: &FramePlan<'_>,
    features: &mut [BorrowedFeature<'_>],
) -> Result<(), FrameError> {
    #[cfg(test)]
    plan_trace::record(false, plan.passes, features, plan.operations);
    chain.encode_frozen(device, encoder, plan, features)
}
