//! The seam through which a game adds its own passes, with its own targets, to the retained
//! renderer's pass list.
//!
//! A game implements [`GameFeatureFactory`] and lists it in
//! [`RendererConfiguration::game_passes`](super::RendererConfiguration::game_passes). The
//! renderer registers the factory's feature on every generation it builds, so the feature takes
//! part in every frame of every route (unshadowed, shadowed and resident) and the host that
//! owns the renderer needs no knowledge of it.
//!
//! The feature is an ordinary [`FrameFeature`](worldrunner_render_post::FrameFeature). It plans
//! passes with [`GameTargets::plan_pass`], which places them in a [`GamePlace`] of the forward
//! frame, attaches the targets the game declared (and optionally the frame's colour and depth)
//! and orders reads after writes through the frame's own validation. In `prepare` it asks
//! [`PassTarget`] for the attachments and depth rules of the entry it records into, prepares its
//! program draws against them, and in `draw` records them into the pass. The frame executor is
//! the one that runs stock passes: nothing here schedules anything of its own.
//!
//! Besides the frame camera, a feature can sample stock products of the frame it draws in: the
//! directional shadow cascades ([`FrameShadowHandle`]) and the screen-space ambient occlusion
//! ([`FrameOcclusionHandle`]). A pass that samples the occlusion declares it with
//! [`GamePassSpec::sampling_frame`], so that frame validation orders the pass after the filter
//! that produces it and the frame computes it even when no stock receiver samples it.
//!
//! [`GamePlace`]: worldrunner_render_post::GamePlace
mod camera;
mod lent;
mod occlusion;
mod pass_target;
mod plan;
mod registry;
mod shadows;
mod targets;

pub use camera::{FrameCamera, FrameCameraHandle};
pub use lent::FrameInputError;
pub use occlusion::{FrameOcclusion, FrameOcclusionHandle, OcclusionDepth};
pub use pass_target::PassTarget;
pub(crate) use plan::reconcile_loads;
pub use plan::{ColorOutput, DepthAccess, GamePassSpec, join_pass};
pub(crate) use registry::Installed;
pub use registry::{GameBuild, GameFeatureFactory, GamePasses};
pub use shadows::{CascadeSelection, FrameShadowHandle, FrameShadows, ShadowCascade};
pub(crate) use targets::TargetStore;
pub use targets::{GameTargets, TargetDeclaration, TargetExtent, TargetLifetime};

impl super::FrameRenderer {
    /// Lends the frame's ambient occlusion to the game's features, when the renderer has any.
    /// Every route calls it as it begins encoding the frame.
    pub(crate) fn publish_game_occlusion(&self) {
        if self.game.features == 0 {
            return;
        }
        self.game.occlusion.publish(
            self.ssao
                .as_ref()
                .map(|metadata| metadata.passes.lend_occlusion()),
        );
    }
}

#[cfg(test)]
mod tests;
