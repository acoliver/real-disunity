//! Bounded observation of ordinary submitted frames.
pub use worldrunner_render_gpu::census::{CensusSnapshot, ResourceClass};
pub use worldrunner_render_gpu::submission::StagingStatistics;
pub use worldrunner_render_scene::draw::{
    DrawCount, EffectEnabled, EffectStatistics, ObjectDraws, ObjectRecording, PassExecution,
    PassInstallation, ReceiverBinding, ReceiverBindings, ResourceStatistics, SceneStatistics,
    ShadowStatistics, SsaoStatistics, TerrainGeometryStatistics, TerrainLeafIdentity,
    TerrainPageStatistics, TerrainStreamingStatistics,
};

/// The latest ordinary submission, available during `Presented` and subsequent updates.
/// Captures and polls do not replace this snapshot. GPU completion is not implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedFrameStatistics {
    pub frame: u64,
    pub generation: u64,
    /// Submitted draws, resources and installed shadow/SSAO receiver state.
    pub scene: SceneStatistics,
    /// Reset applied before this submitted frame, independent of GPU completion.
    pub temporal_history_reset: super::TemporalHistoryReset,
    /// Effective descriptor-byte admission budget for the presentation configuration.
    pub presentation_budget_bytes: u64,
    /// Effective retained capture recipe storage capacity.
    pub capture_pool_bytes: u64,
    /// Resident GPU bytes by resource class, read from the device's memory census when the
    /// frame was submitted. Classes the engine does not count yet read zero.
    pub resident_bytes: CensusSnapshot,
    /// The device's reusable upload staging when the frame was submitted, counted since the
    /// device was created. `chunks_created` stops growing once the frames in flight have
    /// settled: steady frames reuse staging instead of creating buffers.
    pub upload_staging: StagingStatistics,
}
impl super::RetainedHost {
    /// A single replaceable snapshot, initially absent. Reading does not consume it.
    pub fn statistics(&self) -> Option<&RetainedFrameStatistics> {
        self.statistics.as_ref()
    }
}
impl super::RetainedEventHost<'_> {
    pub fn statistics(&self) -> Option<&RetainedFrameStatistics> {
        self.host.statistics()
    }
}
