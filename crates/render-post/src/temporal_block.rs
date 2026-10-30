//! Uniform block of the temporal reconstruction stage.
use crate::{PostError, TaaSettings};

/// Size in bytes of the [`TemporalBlock`], three `vec4<f32>` rows.
pub const TEMPORAL_BLOCK_BYTES: usize = 48;

/// Per-frame data of `taa_upscale.wgsl`. It is separate from the native [`crate::TaaUniform`]:
/// that block carries a reprojection matrix at one extent and this one two extents. Layout,
/// little-endian f32, matching the `Temporal` struct in `taa_upscale.wgsl`:
///
/// | bytes   | content |
/// |---------|---------|
/// | 0..8    | `render` extent `R` x, y in internal pixels |
/// | 8..16   | `output` extent `O` x, y in output pixels |
/// | 16..24  | `jitter` x, y in internal pixels (+x right, +y down) |
/// | 24..28  | `phase_count` of the jitter sequence |
/// | 28..32  | `history_weight` |
/// | 32..36  | `clip_scale` |
/// | 36..40  | `history_valid`: 1.0 when the history holds the previous frame, 0.0 after a reset |
/// | 40..48  | zero padding |
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalBlock {
    pub render: [u32; 2],
    pub output: [u32; 2],
    pub jitter: [f32; 2],
    pub phase_count: u32,
    pub settings: TaaSettings,
    pub history_valid: bool,
}
impl TemporalBlock {
    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] when an extent is zero or above 2^24 (not exact in
    /// f32), a jitter component is not finite or beyond one internal pixel, `phase_count` is
    /// zero, or the settings are invalid.
    pub fn validate(self) -> Result<(), PostError> {
        let extent_ok = |e: [u32; 2]| e.iter().all(|v| (1..=MAX_EXTENT).contains(v));
        let jitter_ok = self.jitter.iter().all(|v| v.is_finite() && v.abs() <= 1.0);
        if extent_ok(self.render) && extent_ok(self.output) && jitter_ok && self.phase_count > 0 {
            self.settings
                .validate()
                .map_err(|_| PostError::UnsupportedProfile(REJECTED))
        } else {
            Err(PostError::UnsupportedProfile(REJECTED))
        }
    }

    /// The block as bytes.
    ///
    /// # Errors
    ///
    /// As [`Self::validate`]; an invalid block never reaches the GPU.
    pub fn to_bytes(self) -> Result<[u8; TEMPORAL_BLOCK_BYTES], PostError> {
        self.validate()?;
        let [rw, rh] = self.render.map(exact);
        let [ow, oh] = self.output.map(exact);
        let rows = [
            rw,
            rh,
            ow,
            oh,
            self.jitter[0],
            self.jitter[1],
            exact(self.phase_count),
            self.settings.history_weight,
            self.settings.clip_scale,
            if self.history_valid { 1.0 } else { 0.0 },
            0.0,
            0.0,
        ];
        let mut bytes = [0; TEMPORAL_BLOCK_BYTES];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(rows) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        Ok(bytes)
    }
}

/// Largest extent or count an f32 holds exactly.
const MAX_EXTENT: u32 = 1 << 24;

const REJECTED: &str = "temporal reconstruction block";

/// `v` as f32; exact for the validated range 0..=2^24.
#[expect(
    clippy::cast_precision_loss,
    reason = "validated to at most 2^24, which f32 represents exactly"
)]
fn exact(v: u32) -> f32 {
    v as f32
}
