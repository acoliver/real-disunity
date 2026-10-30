//! Configuration rows of [`UpscaleMode::Temporal`](crate::UpscaleMode::Temporal).
//!
//! Temporal reconstruction accumulates jittered scene HDR with TAA directly on the output grid.
//! Every combination it cannot serve is a typed error here, before any resource exists.
use crate::{FrameConfig, HdrPostSettings, PostError, ProjectionJitter, UpscaleMode};

impl HdrPostSettings {
    /// The Temporal rows of the configuration table for `config`.
    ///
    /// Settings without `UpscaleMode::Temporal` always pass. Equal extents with TAA are the
    /// native TAA path and pass for any sample count. A reduced extent additionally needs one
    /// sample per pixel, at least half the output on both axes, and an explicit jitter sequence
    /// at least as long as [`ProjectionJitter::for_extents`] derives.
    pub(crate) fn validate_temporal(
        self,
        config: FrameConfig,
        explicit: Option<ProjectionJitter>,
    ) -> Result<(), PostError> {
        if self.upscale.map(|upscale| upscale.mode) != Some(UpscaleMode::Temporal) {
            return Ok(());
        }
        if self.resolve.active_taa().is_none() {
            return unsupported("temporal upscaling requires a TAA resolve");
        }
        let output = config.output_extent();
        if config.size == output {
            return Ok(());
        }
        if config.samples > 1 {
            return unsupported("temporal upscaling requires one sample per pixel");
        }
        if config
            .size
            .iter()
            .zip(output)
            .any(|(&render, output)| u64::from(render) * 2 < u64::from(output))
        {
            return unsupported(
                "temporal upscaling requires a render extent of at least half the output",
            );
        }
        let derived = ProjectionJitter::for_extents(config.size, output)?;
        if explicit.is_some_and(|jitter| jitter.length() < derived.length()) {
            return unsupported(
                "temporal upscaling requires a jitter sequence at least as long as the derived one",
            );
        }
        unsupported("temporal reconstruction is not yet scheduled")
    }
}

fn unsupported(message: &'static str) -> Result<(), PostError> {
    Err(PostError::UnsupportedProfile(message))
}
