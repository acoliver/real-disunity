use super::CaptureRequestError;

/// Deterministic sub-pixel offsets, in output-pixel units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureJitter {
    None,
    Halton23,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOutput {
    Srgb8,
    LinearHdr,
    Depth,
    Normals,
    AmbientOcclusion,
    SsrConfidence,
}

/// Validated capture intent. Unsupported execution modes are rejected at admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedCaptureRecipe {
    pub(crate) supersample: u32,
    pub(crate) temporal_samples: u32,
    pub(crate) jitter: CaptureJitter,
    pub(crate) advance_clock: bool,
    pub(crate) outputs: Vec<CaptureOutput>,
    pub(crate) max_bytes: u64,
    pub(crate) require_pages: bool,
    pub(crate) require_geometry: bool,
    pub(crate) frame_samples: u32,
}
impl RetainedCaptureRecipe {
    pub fn new(
        supersample: u32,
        temporal_samples: u32,
        jitter: CaptureJitter,
        advance_clock: bool,
        outputs: Vec<CaptureOutput>,
        max_bytes: u64,
    ) -> Result<Self, CaptureRequestError> {
        if supersample == 0 || temporal_samples == 0 || outputs.is_empty() || max_bytes == 0 {
            return Err(CaptureRequestError::InvalidRecipe);
        }
        u64::from(supersample)
            .checked_pow(2)
            .and_then(|n| n.checked_mul(u64::from(temporal_samples)))
            .and_then(|n| n.checked_mul(255))
            .ok_or(CaptureRequestError::InvalidRecipe)?;
        for (index, output) in outputs.iter().enumerate() {
            if outputs[..index].contains(output) {
                return Err(CaptureRequestError::InvalidRecipe);
            }
        }
        Ok(Self {
            supersample,
            temporal_samples,
            jitter,
            advance_clock,
            outputs,
            max_bytes,
            require_pages: false,
            require_geometry: false,
            frame_samples: 1,
        })
    }
    /// Wait for current-view material pages using retained page-residency readiness.
    #[must_use]
    pub fn with_page_dependency(mut self) -> Self {
        self.require_pages = true;
        self
    }
    /// Wait for source-generation geometry at the requested raster footprint.
    #[must_use]
    pub fn with_geometry_dependency(mut self) -> Self {
        self.require_geometry = true;
        self
    }
    pub fn supersample(&self) -> u32 {
        self.supersample
    }
    pub fn temporal_samples(&self) -> u32 {
        self.temporal_samples
    }
    pub fn jitter(&self) -> CaptureJitter {
        self.jitter
    }
    pub fn advances_clock(&self) -> bool {
        self.advance_clock
    }
    pub fn outputs(&self) -> &[CaptureOutput] {
        &self.outputs
    }
    /// Centered Halton bases 2 and 3, indexed from one; offsets use output pixels.
    pub fn jitter_offset(&self, sample: u32) -> [f32; 2] {
        assert!(sample < self.temporal_samples);
        match self.jitter {
            CaptureJitter::None => [0.0; 2],
            CaptureJitter::Halton23 => [
                radical_inverse(sample + 1, 2) - 0.5,
                radical_inverse(sample + 1, 3) - 0.5,
            ],
        }
    }
    /// Advancing recipes use an explicit deterministic 60 Hz subframe clock.
    pub fn subframe_time(&self, base: std::time::Duration, sample: u32) -> std::time::Duration {
        assert!(sample < self.temporal_samples);
        base + if self.advance_clock {
            std::time::Duration::from_nanos(u64::from(sample) * 1_000_000_000 / 60)
        } else {
            std::time::Duration::ZERO
        }
    }
    pub(crate) fn accumulated(&self) -> bool {
        self.supersample != 1
            || self.temporal_samples != 1
            || self.jitter != CaptureJitter::None
            || self.advance_clock
    }
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub(crate) fn render_size(&self, size: [u32; 2]) -> Result<[u32; 2], CaptureRequestError> {
        Ok([
            size[0]
                .checked_mul(self.supersample)
                .ok_or(CaptureRequestError::InvalidRecipe)?,
            size[1]
                .checked_mul(self.supersample)
                .ok_or(CaptureRequestError::InvalidRecipe)?,
        ])
    }

    pub(crate) fn storage_bytes(&self, size: [u32; 2]) -> Result<u64, CaptureRequestError> {
        if size.contains(&0) {
            return Err(CaptureRequestError::InvalidRecipe);
        }
        let output_size = size;
        let size = self.render_size(size)?;
        let mut bytes = 0_u64;
        for output in &self.outputs {
            bytes = bytes
                .checked_add(output_storage(*output, size, self.frame_samples)?)
                .ok_or(CaptureRequestError::InvalidRecipe)?;
        }
        if self.accumulated() {
            let accumulation = u64::from(output_size[0])
                .checked_mul(u64::from(output_size[1]))
                .and_then(|n| n.checked_mul(48))
                .and_then(|n| {
                    n.checked_mul(u64::try_from(self.outputs.len()).expect("output count"))
                })
                .ok_or(CaptureRequestError::InvalidRecipe)?;
            let offscreen = u64::from(size[0]) * u64::from(size[1]) * 4;
            bytes = bytes
                .checked_add(offscreen)
                .and_then(|n| n.checked_add(accumulation))
                .ok_or(CaptureRequestError::InvalidRecipe)?;
        }
        if bytes > self.max_bytes {
            return Err(CaptureRequestError::RecipeBudget {
                required: bytes,
                limit: self.max_bytes,
            });
        }
        Ok(bytes)
    }
}

fn output_storage(
    output: CaptureOutput,
    size: [u32; 2],
    samples: u32,
) -> Result<u64, CaptureRequestError> {
    let width = u64::from(size[0])
        .checked_mul(if output == CaptureOutput::SsrConfidence {
            u64::from(samples)
        } else {
            1
        })
        .ok_or(CaptureRequestError::InvalidRecipe)?;
    let (texel_bytes, cpu_bytes) = match output {
        CaptureOutput::Srgb8 => (4, 4),
        CaptureOutput::LinearHdr | CaptureOutput::Normals => (8, 24),
        CaptureOutput::SsrConfidence => (8, 12),
        CaptureOutput::Depth | CaptureOutput::AmbientOcclusion => (4, 8),
    };
    let row = width
        .checked_mul(texel_bytes)
        .ok_or(CaptureRequestError::InvalidRecipe)?;
    // Staging, mapped copy, tightly packed bytes and decoded pixels.
    row.next_multiple_of(256)
        .checked_mul(2)
        .and_then(|n| n.checked_add(width.checked_mul(cpu_bytes)?))
        .and_then(|n| n.checked_mul(u64::from(size[1])))
        .ok_or(CaptureRequestError::InvalidRecipe)
}

fn radical_inverse(mut index: u32, base: u32) -> f32 {
    let mut value = 0.0_f64;
    let mut weight = 1.0 / f64::from(base);
    while index != 0 {
        value += f64::from(index % base) * weight;
        index /= base;
        weight /= f64::from(base);
    }
    glam::DVec2::splat(value).as_vec2().x
}

/// Requested capture products. Feature targets retain their native raster extents.
/// Feature readbacks currently require one unjittered, non-accumulated frame.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RetainedCaptureProducts {
    pub srgb8: Option<worldrunner_render_scene::offscreen::RgbaImage>,
    pub linear_hdr: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    /// Positive pixel-centre view-space metres before water.
    pub depth: Option<worldrunner_render_scene::offscreen::ScalarImage>,
    /// View-space XYZ normals; alpha is coverage, zero for uncovered pixels.
    pub normals: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    /// Filtered ambient visibility in [0, 1], decoded from the target red channel.
    pub ambient_occlusion: Option<worldrunner_render_scene::offscreen::ScalarImage>,
    /// Confidence in [0, 1] from trace alpha, at native trace resolution.
    /// The returned extent is `(trace_width * frame_sample_count, trace_height)`.
    /// Samples occupy contiguous horizontal lanes: `packed_x = sample * trace_width + x`,
    /// where `trace_width = returned_width / frame_sample_count` and the sample count
    /// comes from the frame profile. Trace dimensions match the raster extent at
    /// full resolution, or its ceil-divided halves at half resolution. Read pixel
    /// `(x, y)` of a sample at `pixels[y * returned_width + packed_x]`.
    pub ssr_confidence: Option<worldrunner_render_scene::offscreen::ScalarImage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipes_reject_empty_duplicate_and_over_budget_outputs() {
        let make = |outputs, max| {
            RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, outputs, max)
        };
        assert!(make(vec![], 100).is_err());
        assert!(make(vec![CaptureOutput::Srgb8; 2], 100).is_err());
        let recipe = make(vec![CaptureOutput::LinearHdr], 1).unwrap();
        assert!(matches!(
            recipe.storage_bytes([64, 64]),
            Err(CaptureRequestError::RecipeBudget { .. })
        ));
        assert!(recipe.storage_bytes([u32::MAX, u32::MAX]).is_err());
    }
}
