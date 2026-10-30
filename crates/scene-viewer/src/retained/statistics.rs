//! Bounded observation of ordinary submitted frames.
pub use worldrunner_render_scene::draw::{DrawCount, ResourceStatistics, SceneStatistics};

/// The latest ordinary submission, available during `Presented` and subsequent updates.
/// Captures and polls do not replace this snapshot. GPU completion is not implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedFrameStatistics {
    pub frame: u64,
    pub generation: u64,
    pub scene: SceneStatistics,
    /// Effective descriptor-byte admission budget for the presentation configuration.
    pub presentation_budget_bytes: u64,
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
