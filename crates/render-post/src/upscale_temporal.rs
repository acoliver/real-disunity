//! Configuration rows of [`UpscaleMode::Temporal`](crate::UpscaleMode::Temporal).
//!
//! Temporal reconstruction accumulates jittered scene HDR with TAA directly on the output grid.
//! Every combination it cannot serve is a typed error here, before any resource exists.
use crate::{FrameConfig, HdrPostSettings, PostError, ProjectionJitter, UpscaleMode};

impl HdrPostSettings {
    /// The extent of the colour the post chain carries: the output when TAA reconstructs onto the
    /// output grid, the render extent for every other mode.
    pub(crate) fn post_extent(self, config: FrameConfig) -> [u32; 2] {
        if self.reconstructs_temporally(config) {
            config.output_extent()
        } else {
            config.size
        }
    }

    /// RCAS after temporal reconstruction. It runs on the display-linear image at the output
    /// extent like the native RCAS, because there is no spatial upscale to fold it into.
    pub(crate) fn temporal_rcas(self, config: FrameConfig) -> Option<crate::NativeRcasSettings> {
        self.upscale
            .filter(|_| self.reconstructs_temporally(config))
            .and_then(|upscale| upscale.sharpness)
            .map(|stops| crate::NativeRcasSettings {
                stops,
                domain: crate::RcasDomain::DisplayLinear,
            })
    }

    /// Whether TAA reconstructs the output grid itself: `UpscaleMode::Temporal` at a render
    /// extent that differs from the output. Every other configuration, including Temporal at
    /// equal extents (the native TAA), keeps the render grid for the whole post chain.
    pub fn reconstructs_temporally(self, config: FrameConfig) -> bool {
        self.upscale
            .is_some_and(|upscale| upscale.mode == UpscaleMode::Temporal)
            && config.size != config.output_extent()
    }

    /// The material mip bias a renderer with these settings applies at `config`: exactly `+0.0`
    /// in every mode.
    ///
    /// The sampling plumbing exists (the frame lane and every biased site), but no nonzero
    /// policy ships. The candidates `log2(s) - 1`, `log2(s) - 0.5` and `log2(s)`, with
    /// `s = min(Rw / Ow, Rh / Oh)`, lowered the detail error at every tested scale. On the AMD
    /// Radeon 780M at render scale 0.5 every one of them exceeded 1.10 times the native TAA
    /// shimmer (1.239, 1.181 and 1.159 on the stripe fixture). The decision rule required a
    /// candidate to pass at every scale on every tested device, so the policy is 0. The tables
    /// and the rule are in the "Material mip bias" section of docs/rendering.md. A later change
    /// can switch a policy on after the accumulation work (locks, reactive masks), measured
    /// again on both devices.
    #[must_use]
    pub fn material_mip_bias(self, _config: FrameConfig) -> f32 {
        0.0
    }

    /// The projection jitter a renderer with these settings uses at `config`, given the explicitly
    /// requested one.
    ///
    /// # Errors
    ///
    /// The error of [`ProjectionJitter::for_extents`] when the extents cannot be reconstructed.
    pub fn effective_jitter(
        self,
        config: FrameConfig,
        explicit: Option<ProjectionJitter>,
    ) -> Result<Option<ProjectionJitter>, PostError> {
        if explicit.is_none()
            && self.resolve.active_taa().is_some()
            && self.reconstructs_temporally(config)
        {
            return ProjectionJitter::for_extents(config.size, config.output_extent()).map(Some);
        }
        Ok(self.resolve.effective_jitter(explicit))
    }

    /// Whether a renderer at `config` may use `jitter` as its projection jitter.
    ///
    /// Reduced temporal reconstruction needs a sequence at least as long as
    /// [`ProjectionJitter::for_extents`] derives; every other configuration accepts any sequence,
    /// and `None` means the renderer derives its own.
    ///
    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] for a shorter sequence, or the error of
    /// [`ProjectionJitter::for_extents`] when the extents cannot be reconstructed.
    pub fn validate_jitter(
        self,
        config: FrameConfig,
        jitter: Option<ProjectionJitter>,
    ) -> Result<(), PostError> {
        let Some(jitter) = jitter.filter(|_| self.reconstructs_temporally(config)) else {
            return Ok(());
        };
        let derived = ProjectionJitter::for_extents(config.size, config.output_extent())?;
        if jitter.length() < derived.length() {
            return unsupported(JITTER_TOO_SHORT);
        }
        Ok(())
    }

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
        self.validate_jitter(config, explicit)
    }
}

const JITTER_TOO_SHORT: &str =
    "temporal upscaling requires a jitter sequence at least as long as the derived one";

fn unsupported(message: &'static str) -> Result<(), PostError> {
    Err(PostError::UnsupportedProfile(message))
}
