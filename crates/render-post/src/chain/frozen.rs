//! The frozen-plan entry of the post chain.
use super::PostChain;
use crate::StageFactory;

impl<F: StageFactory> PostChain<F> {
    /// The declarations validation reads: borrowed resident features precede retained
    /// registrations.
    pub(super) fn declarations(
        &self,
        borrowed: &[crate::BorrowedFeature<'_>],
    ) -> Vec<crate::FeatureDeclaration> {
        borrowed
            .iter()
            .map(|feature| feature.declaration.clone())
            .chain(
                self.features
                    .iter()
                    .map(|feature| feature.declaration.clone()),
            )
            .collect()
    }

    /// Encodes a frozen plan exactly as given: nothing adds, removes or sorts its entries.
    ///
    /// The plan is admitted before any feature prepares. It meets the contract checks of
    /// [`Self::encode_geometry`], and also the orders the stock passes need, which a plan built
    /// by the stock builder satisfies by construction and a caller's plan can break: contours
    /// and reflections after every Opaque entry, and a depth read (listed or tested read-only)
    /// only once the depth is complete.
    ///
    /// # Errors
    /// [`crate::FrameError::StockOrder`] or [`crate::FrameError::IncompleteDepth`] for those
    /// orders, then the errors of
    /// [`Self::encode_geometry`], all before any feature prepares.
    pub fn encode_frozen(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &crate::FramePlan<'_>,
        borrowed: &mut [crate::BorrowedFeature<'_>],
    ) -> Result<(), crate::FrameError> {
        let declarations = self.declarations(borrowed);
        crate::frame::validate_stock_order(plan.passes, &declarations)?;
        crate::frame::validate_plan(
            plan.passes,
            &declarations,
            plan.requested,
            plan.views,
            plan.operations,
        )?;
        self.encode_validated(device, encoder, plan, borrowed, &declarations)
    }
}
