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
/// Offsets are in raster pixels of the internal render target (the output pixels when no
/// upscaling happens), strictly inside `(-0.5, 0.5)` on both axes, with +x to the
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

    /// The sequence for a scene rasterised at `render` extent and reconstructed to `output`.
    ///
    /// The length is `ceil(8 * max(Ow / Rw, Oh / Rh)^2)`: eight phases at equal extents, and
    /// more as the output holds more pixels per raster pixel, so each output pixel still sees
    /// about eight distinct sample positions. It is computed per axis as
    /// `ceil(8 * O^2 / R^2)` in integers, so a ratio that lands exactly on an integer is never
    /// pushed up by float rounding. Equal extents give exactly [`Self::default`]. Offsets stay
    /// in raster pixels of the `render` extent, as [`Self::offset_pixels`] documents.
    ///
    /// # Errors
    ///
    /// [`PostError::EmptyExtent`] when either extent has a zero side,
    /// [`PostError::RenderExtent`] when `render` exceeds `output` on an axis, and
    /// [`PostError::UnsupportedProfile`] when `render` is below half of `output` on an axis
    /// (the limit of `worldrunner_render_plan::post::RenderScale`) or the length would exceed [`Self::MAX_LENGTH`].
    pub fn for_extents(render: [u32; 2], output: [u32; 2]) -> Result<Self, PostError> {
        if render.contains(&0) || output.contains(&0) {
            return Err(PostError::EmptyExtent);
        }
        if render.iter().zip(output).any(|(&r, o)| r > o) {
            return Err(PostError::RenderExtent);
        }
        if render
            .iter()
            .zip(output)
            .any(|(&r, o)| u64::from(r) * 2 < u64::from(o))
        {
            return Err(PostError::UnsupportedProfile(
                "render extent must be at least half of the output extent",
            ));
        }
        let phases = render
            .into_iter()
            .zip(output)
            .map(|(r, o)| phases_for_axis(r, o))
            .max()
            .expect("two axes");
        Self::new(u32::try_from(phases).unwrap_or(u32::MAX))
    }

    /// Number of frames before the offsets repeat.
    pub const fn length(self) -> u32 {
        self.length
    }

    /// Sub-pixel offset of the projection for `frame_index`, in internal raster pixels.
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

    /// The offset the scene passes apply for a render target of `samples` samples per pixel.
    ///
    /// A multisampled frame is already box filtered over the pixel: its pixels hold area
    /// averages, and accumulating those at shifted positions converges to the box convolved with
    /// the jitter distribution, an image blurrier than the multisampled frame itself (static 4x
    /// error 0.0015 against 0.00046 measured, whichever way the shifted pixel is reconstructed).
    /// The offset is therefore zero for more than one sample: TAA then stabilises in time
    /// and adds no spatial supersampling.
    pub fn offset_pixels_for(self, frame_index: u64, samples: u32) -> [f32; 2] {
        if samples > 1 {
            [0.0; 2]
        } else {
            self.offset_pixels(frame_index)
        }
    }

    /// The same offset as a translation of clip space for a viewport of `size` pixels.
    ///
    /// Clip x grows to the right and clip y upwards, so the vertical pixel offset changes sign.
    /// This is the translation `ResolvedView::with_raster_sample` applies to a projection.
    pub fn clip_offset(self, frame_index: u64, size: [u32; 2]) -> [f32; 2] {
        Self::clip_translation(self.offset_pixels(frame_index), size)
    }

    /// [`Self::clip_offset`] for a render target of `samples` samples per pixel, as
    /// [`Self::offset_pixels_for`].
    pub fn clip_offset_for(self, frame_index: u64, size: [u32; 2], samples: u32) -> [f32; 2] {
        Self::clip_translation(self.offset_pixels_for(frame_index, samples), size)
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "viewport extents are far below 2^24"
    )]
    fn clip_translation([x, y]: [f32; 2], size: [u32; 2]) -> [f32; 2] {
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

/// `ceil(8 * output^2 / render^2)` for one axis, exact for every `u32` extent.
fn phases_for_axis(render: u32, output: u32) -> u128 {
    let (render, output) = (u128::from(render), u128::from(output));
    let square = render * render;
    (u128::from(ProjectionJitter::DEFAULT_LENGTH) * output * output).div_ceil(square)
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
