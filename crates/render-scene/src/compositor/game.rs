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
//! It can also sample the installed specular environment cube ([`FrameEnvironmentHandle`]), a
//! retained texture that needs no declaration.
//!
//! A pass that draws over the opaque scene with a copy of it, as a water surface does, is placed
//! at [`GamePlace::AfterOpaqueSnapshots`] and declares the snapshots it samples with
//! [`GamePassSpec::reading_snapshots`]. [`GameFeatureFactory::snapshot_reads`] reserves them and
//! [`FrameSnapshotHandle`] lends them to the frame that wrote them.
//!
//! [`GamePlace`]: worldrunner_render_post::GamePlace
//! [`GamePlace::AfterOpaqueSnapshots`]: worldrunner_render_post::GamePlace::AfterOpaqueSnapshots
mod camera;
mod environment;
mod frame_start;
mod lent;
mod occlusion;
mod pass_target;
mod plan;
mod registry;
mod shadows;
mod snapshots;
mod targets;
mod terrain;

pub use camera::{FrameCamera, FrameCameraHandle};
pub use environment::{EnvironmentConsumer, FrameEnvironment, FrameEnvironmentHandle};
pub use lent::FrameInputError;
pub use occlusion::{FrameOcclusion, FrameOcclusionHandle, OcclusionDepth};
pub use pass_target::PassTarget;
pub use plan::{ColorOutput, DepthAccess, GamePassSpec, join_pass, prepass_writes_frame_depth};
pub(crate) use plan::{place_after_prepass, reconcile_loads};
pub(crate) use registry::Installed;
pub use registry::{GameBuild, GameFeatureFactory, GamePasses};
pub use shadows::{CascadeSelection, FrameShadowHandle, FrameShadows, ShadowCascade};
pub use snapshots::{
    FrameSnapshotError, FrameSnapshotHandle, FrameSnapshots, PostWaterDepth, SnapshotImage,
    SnapshotReads, SnapshotStatistics,
};
pub(crate) use targets::TargetStore;
pub use targets::{GameTargets, TargetDeclaration, TargetExtent, TargetLifetime};
#[cfg(test)]
pub(crate) use terrain::probe as terrain_probe;
pub use terrain::{FrameTerrainError, FrameTerrainHandle};

impl super::FrameRenderer {
    /// Lends the frame's ambient occlusion and specular environment to the game's features,
    /// when the renderer has any. Every route calls it as it begins encoding the frame.
    pub(crate) fn publish_game_inputs(&self) {
        if self.game.features == 0 {
            return;
        }
        self.game.occlusion.publish(
            self.ssao
                .as_ref()
                .map(|metadata| metadata.passes.lend_occlusion()),
        );
        self.game.environment.publish(self.lend_environment());
    }
}

#[cfg(test)]
mod frame_start_tests;
#[cfg(test)]
mod tests;
