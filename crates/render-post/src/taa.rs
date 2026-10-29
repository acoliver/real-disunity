//! Temporal antialiasing settings.
//!
//! TAA accumulates jittered frames into a history image. It runs on the scene-referred
//! color before metering, bloom and tone mapping, so it needs no display-referred
//! intermediate and adaptive exposure never ghosts into the history.
use crate::PostError;

/// Longest accepted history weight. A weight of 1 would never admit a new sample.
const MAX_HISTORY_WEIGHT: f32 = 0.99;

/// Accepted scale of the 3x3 neighborhood min/max color box.
const CLIP_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;

/// Temporal antialiasing controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaaSettings {
    /// Weight of the reprojected history in the blend, finite and within 0..=0.99. Larger
    /// values smooth more and respond more slowly; 0 keeps only the current frame.
    pub history_weight: f32,
    /// Scale of the neighborhood color box that history is clipped to, finite and within
    /// 0.5..=2. The box is the min/max of the 3x3 neighborhood in luminance-compressed
    /// `YCoCg`, scaled about its centre: 1 keeps the box as measured, smaller values reject
    /// more ghosting and larger ones admit more history.
    pub clip_scale: f32,
}
impl TaaSettings {
    /// A 90 percent history with the neighborhood min/max clip box at scale 1.
    pub const DEFAULT: Self = Self {
        history_weight: 0.9,
        clip_scale: 1.0,
    };

    /// The default clip box with the given history weight.
    pub const fn new(history_weight: f32) -> Self {
        Self {
            history_weight,
            ..Self::DEFAULT
        }
    }

    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] when a value is not finite or is out of range.
    pub fn validate(self) -> Result<(), PostError> {
        let weight_ok = self.history_weight.is_finite()
            && (0.0..=MAX_HISTORY_WEIGHT).contains(&self.history_weight);
        let clip_ok = self.clip_scale.is_finite() && CLIP_SCALE_RANGE.contains(&self.clip_scale);
        if weight_ok && clip_ok {
            Ok(())
        } else {
            Err(PostError::UnsupportedProfile("TAA settings"))
        }
    }
}
impl Default for TaaSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Size in bytes of the [`TaaUniform`] block.
pub const TAA_UNIFORM_BYTES: usize = 80;

/// Per-frame data the TAA resolve pass reads from a uniform buffer.
///
/// The host computes the reprojection in f64, where inverting a view-projection matrix with
/// a long far plane stays accurate, and this block carries it to the GPU as f32. Layout,
/// little-endian, matching the `Taa` struct in `taa.wgsl`:
///
/// | bytes   | content |
/// |---------|---------|
/// | 0..64   | `reprojection`, column-major `mat4x4<f32>` |
/// | 64..72  | `jitter_pixels` x, y (+y down) |
/// | 72..76  | `history_valid`: 1.0 when the history texture holds the previous frame, else 0.0 |
/// | 76..80  | zero padding |
///
/// An all-zero block reads as "no history": the resolve then returns the reconstructed
/// current color without touching the history texture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaaUniform {
    /// Previous unjittered view-projection times the inverse of the current unjittered
    /// view-projection, as columns (`m[column][row]`) applied to column vectors. It maps
    /// (NDC x, NDC y, device depth 0..1, 1) of the current frame to the previous clip space.
    pub reprojection: [[f64; 4]; 4],
    /// Projection jitter of the current frame in pixels, +x right and +y down.
    pub jitter_pixels: [f32; 2],
    pub history_valid: bool,
}
impl TaaUniform {
    pub fn to_bytes(self) -> [u8; TAA_UNIFORM_BYTES] {
        let mut bytes = [0; TAA_UNIFORM_BYTES];
        let matrix = self.reprojection.iter().flatten().map(|v| narrow(*v));
        let tail = [
            self.jitter_pixels[0],
            self.jitter_pixels[1],
            if self.history_valid { 1.0 } else { 0.0 },
            0.0,
        ];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(matrix.chain(tail)) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

#[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
fn narrow(v: f64) -> f32 {
    v as f32
}
