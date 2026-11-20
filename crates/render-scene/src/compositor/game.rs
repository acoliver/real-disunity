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
//! A pass that draws over the opaque scene with a copy of it, as a water surface does, is placed
//! at [`GamePlace::AfterOpaqueSnapshots`] and declares the snapshots it samples with
//! [`GamePassSpec::reading_snapshots`]. [`GameFeatureFactory::snapshot_reads`] reserves them and
//! [`FrameSnapshotHandle`] lends them to the frame that wrote them.
//!
//! [`GamePlace`]: worldrunner_render_post::GamePlace
//! [`GamePlace::AfterOpaqueSnapshots`]: worldrunner_render_post::GamePlace::AfterOpaqueSnapshots
mod camera;
mod frame_start;
mod pass_target;
mod plan;
mod registry;
mod shadows;
mod snapshots;
mod targets;
mod terrain;

pub use camera::{FrameCamera, FrameCameraHandle};
pub use pass_target::PassTarget;
pub(crate) use plan::reconcile_loads;
pub use plan::{ColorOutput, DepthAccess, GamePassSpec, join_pass};
pub(crate) use registry::Installed;
pub use registry::{GameBuild, GameFeatureFactory, GamePasses};
pub use shadows::{
    CascadeSelection, FrameShadowError, FrameShadowHandle, FrameShadows, ShadowCascade,
};
pub use snapshots::{
    FrameSnapshotError, FrameSnapshotHandle, FrameSnapshots, SnapshotImage, SnapshotReads,
    SnapshotStatistics,
};
pub(crate) use targets::TargetStore;
pub use targets::{GameTargets, TargetDeclaration, TargetExtent, TargetLifetime};
#[cfg(test)]
pub(crate) use terrain::probe as terrain_probe;
pub use terrain::{FrameTerrainError, FrameTerrainHandle};

#[cfg(test)]
mod frame_start_tests;
#[cfg(test)]
mod tests;
