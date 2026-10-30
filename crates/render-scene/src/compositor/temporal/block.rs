//! The block of reduced temporal reconstruction and the extents its history is keyed by.
//!
//! The block is built on the CPU like the native TAA block. A frame without usable history
//! (the first frame, a camera cut, any resize or settings change) uploads a real block with
//! `history_valid` false: an all-zero block has zero extents, which the shader divides by.
use crate::frame::temporal::TemporalView;
use worldrunner_render_post::{FrameConfig, ProjectionJitter, TaaSettings, TemporalBlock};

/// The temporal block of a frame: `view` is the frame's camera data, `None` for a frame that
/// has no temporal data and therefore no history.
pub(super) fn temporal_block(
    config: FrameConfig,
    jitter: ProjectionJitter,
    settings: TaaSettings,
    view: Option<&TemporalView>,
) -> TemporalBlock {
    TemporalBlock {
        render: config.size,
        output: config.output_extent(),
        jitter: view.map_or([0.0; 2], |view| view.jitter_pixels),
        phase_count: jitter.length(),
        settings,
        history_valid: view.is_some_and(|view| view.history_valid),
    }
}

/// The extents a published frame is valid for: history from one pair of render and output
/// extents never feeds a frame at another.
pub(super) fn extents_key(config: FrameConfig) -> [[u32; 2]; 2] {
    [config.size, config.output_extent()]
}

#[cfg(test)]
#[path = "block_tests.rs"]
mod tests;
