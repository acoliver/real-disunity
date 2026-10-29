//! Sub-pixel projection jitter for temporal accumulation.
//!
//! A temporal resolve needs a different sample position inside the pixel on each frame.
//! The offsets here follow the Halton(2,3) low-discrepancy sequence, which covers the pixel
//! evenly for any prefix length. This module is pure arithmetic: it never touches a camera,
//! a matrix or a GPU resource, so the offsets can be unit tested exactly.

use crate::PostError;

/// First prime base of the sequence, driving the horizontal offset.
const BASE_X: u32 = 2;

/// Second prime base of the sequence, driving the vertical offset.
const BASE_Y: u32 = 3;

/// A Halton(2,3) sub-pixel jitter sequence that repeats after `length` frames.
///
/// Offsets are in output pixels, strictly inside `(-0.5, 0.5)` on both axes, with +x to the
/// right and +y down the image, the convention of screen coordinates. The sequence starts at
/// Halton index 1: index 0 is the pixel corner, which is not strictly inside the open interval.
///
/// Jitter changes the image every frame. Use it only together with a temporal resolve that
/// accumulates the frames; without one the picture shimmers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionJitter {
    length: u32,
}

impl ProjectionJitter {
    /// Sequence length used by [`Default`].
    pub const DEFAULT_LENGTH: u32 = 8;

    /// Longest accepted sequence.
    pub const MAX_LENGTH: u32 = 64;

    /// A sequence of `length` frames, from 1 to [`Self::MAX_LENGTH`].
    ///
    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] when `length` is outside that range.
    pub fn new(length: u32) -> Result<Self, PostError> {
        if !(1..=Self::MAX_LENGTH).contains(&length) {
            return Err(PostError::UnsupportedProfile(
                "projection jitter length must be within 1..=64",
            ));
        }
        Ok(Self { length })
    }

    /// Number of frames before the offsets repeat.
    pub const fn length(self) -> u32 {
        self.length
    }

    /// Sub-pixel offset of the projection for `frame_index`, in output pixels.
    pub fn offset_pixels(self, frame_index: u64) -> [f32; 2] {
        let point = u32::try_from(frame_index % u64::from(self.length))
            .expect("the remainder is below a u32 length")
            + 1;
        [BASE_X, BASE_Y].map(|base| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a value in (-0.5, 0.5) is exact enough in f32"
            )]
            let offset = (halton(point, base) - 0.5) as f32;
            offset
        })
    }

    /// The same offset as a translation of clip space for a viewport of `size` pixels.
    ///
    /// Clip x grows to the right and clip y upwards, so the vertical pixel offset changes sign.
    /// This is the translation `ResolvedView::with_raster_sample` applies to a projection.
    #[expect(
        clippy::cast_precision_loss,
        reason = "viewport extents are far below 2^24"
    )]
    pub fn clip_offset(self, frame_index: u64, size: [u32; 2]) -> [f32; 2] {
        let [x, y] = self.offset_pixels(frame_index);
        [x * 2.0 / size[0] as f32, -y * 2.0 / size[1] as f32]
    }
}

impl Default for ProjectionJitter {
    fn default() -> Self {
        Self {
            length: Self::DEFAULT_LENGTH,
        }
    }
}

/// The radical inverse of `index` in `base`, in `[0, 1)`.
fn halton(mut index: u32, base: u32) -> f64 {
    let (mut result, mut fraction) = (0.0, 1.0);
    while index > 0 {
        fraction /= f64::from(base);
        result += fraction * f64::from(index % base);
        index /= base;
    }
    result
}
