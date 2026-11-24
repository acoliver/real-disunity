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
//! retained texture that needs no declaration, and the scene's resident textures
//! ([`FrameSceneTexturesHandle`]).
//!
//! The feature owns no GPU resource. The buffers, textures and samplers its passes read are
//! declared to [`GameResources`], which the generation creates and owns, and its programs, sets
//! and draws are made with that store's program runtime.
//!
//! A pass that draws over the opaque scene with a copy of it, as a water surface does, is placed
//! at [`GamePlace::AfterOpaqueSnapshots`] and declares the snapshots it samples with
//! [`GamePassSpec::reading_snapshots`]. [`GameFeatureFactory::snapshot_reads`] reserves them and
//! [`FrameSnapshotHandle`] lends them to the frame that wrote them.
//!
//! A pass that draws over the finished scene colour, as blended particles do, is placed at
//! [`GamePlace::BeforePost`], after every other writer of the colour and before the post chain
//! reads it, and blends with the colour states it sets on the [`PassTarget::render_state`] it
//! draws with. With [`DepthAccess::ReadOnlySampled`] it tests the frame's
//! depth and samples the same depth in that pass ([`PassTarget::sampled_depth`]).
//!
//! [`GamePlace`]: worldrunner_render_post::GamePlace
//! [`GamePlace::AfterOpaqueSnapshots`]: worldrunner_render_post::GamePlace::AfterOpaqueSnapshots
//! [`GamePlace::BeforePost`]: worldrunner_render_post::GamePlace::BeforePost
mod camera;
mod compute;
mod declared_textures;
#[cfg(test)]
mod declaring;
mod engine;
mod environment;
mod frame_start;
mod lent;
mod level_import;
mod occlusion;
mod pass_target;
mod plan;
mod registry;
mod replacement;
mod resources;
mod scene_meshes;
mod scene_textures;
mod shadows;
mod snapshots;
mod targets;
mod terrain;

pub use camera::FrameCamera;
pub use camera::FrameCameraHandle;
pub use compute::GameComputeSpec;
pub use declared_textures::GameTextureDeclaration;
pub use declared_textures::GameTextureLevel;
#[cfg(test)]
pub(crate) use declaring::declaring_stock;
pub use engine::BRDF_TABLE_SAMPLES;
pub use engine::BRDF_TABLE_SIZE;
pub use engine::EngineConstants;
pub use engine::EnginePlaceholder;
pub use engine::FULLSCREEN_TRIANGLE_WGSL;
pub use engine::LayerShape;
pub(crate) use engine::SceneSources;
pub use engine::TerrainMaterialArrayError;
pub use engine::TerrainMaterialChannel;
pub use environment::EnvironmentConsumer;
pub use environment::FrameEnvironment;
pub use environment::FrameEnvironmentHandle;
pub use lent::FrameInputError;
pub use level_import::LevelAccess;
pub use occlusion::FrameOcclusion;
pub use occlusion::FrameOcclusionHandle;
pub use occlusion::OcclusionDepth;
pub use pass_target::PassTarget;
pub use plan::ColorOutput;
pub use plan::DepthAccess;
pub use plan::GamePassSpec;
pub use plan::join_pass;
pub(crate) use plan::place_after_prepass;
pub use plan::prepass_writes_frame_depth;
pub(crate) use plan::reconcile_loads;
pub use registry::GameBuild;
pub use registry::GameFeatureFactory;
pub use registry::GamePasses;
pub(crate) use registry::Installed;
pub use replacement::StockReplacement;
pub use resources::GameBlockDestination;
pub use resources::GameBuffer;
pub use resources::GameBufferDeclaration;
pub use resources::GameBufferUsage;
pub use resources::GameLevel;
pub use resources::GameReadback;
pub use resources::GameResourceError;
pub use resources::GameResources;
pub use resources::GameSampler;
pub use resources::GameTexture;
pub use scene_meshes::FrameSceneMeshError;
pub use scene_meshes::FrameSceneMeshes;
pub use scene_meshes::FrameSceneMeshesHandle;
pub use scene_meshes::SceneMeshSlot;
pub use scene_textures::FrameSceneTextureError;
pub use scene_textures::FrameSceneTextures;
pub use scene_textures::FrameSceneTexturesHandle;
pub use scene_textures::FrameWaterNormalError;
pub use shadows::CascadeSelection;
pub use shadows::FrameShadowHandle;
pub use shadows::FrameShadows;
pub use shadows::ShadowCascade;
pub use snapshots::FrameSnapshotError;
pub use snapshots::FrameSnapshotHandle;
pub use snapshots::FrameSnapshots;
pub use snapshots::PostWaterDepth;
pub use snapshots::SnapshotImage;
pub use snapshots::SnapshotReads;
pub use snapshots::SnapshotStatistics;
pub use targets::GameTargets;
pub use targets::TargetDeclaration;
pub use targets::TargetExtent;
pub use targets::TargetLifetime;
pub(crate) use targets::TargetStore;
pub use terrain::FrameTerrainError;
pub use terrain::FrameTerrainHandle;
#[cfg(test)]
pub(crate) use terrain::probe as terrain_probe;

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

    /// Records the block transfers and readbacks the game's features asked of their resources
    /// into `encoder`'s frame. Every route calls it after the passes it encodes, so the work
    /// lands after the frame's (or the depth prefix's) passes and before its submission. A depth
    /// prefix submitted ahead of the frame records everything queued by then, so that work lands
    /// before the rest of the frame's passes.
    ///
    /// # Errors
    /// [`FrameError`](worldrunner_render_post::FrameError) when a transfer is refused or
    /// `encoder` is not a frame being recorded.
    pub(crate) fn record_game_work(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        for resources in &self.game.resources {
            resources.record_work(encoder)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod frame_start_tests;
#[cfg(test)]
mod inputs_tests;
#[cfg(test)]
mod tests;
