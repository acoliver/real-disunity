//! Answering a script's read of a rectangle of a render texture.
//!
//! The world lists an `OffscreenWork::ReadPixels` where the script asked, and the flush runs it
//! after the work listed before it: the work up to it is submitted with a copy of the render
//! texture's first mip recorded last, the copy is read when the submission is done, and the
//! rectangle comes out as four floats a texel (red, green, blue, alpha) as the texture stores
//! them, with no decoding, so an sRGB texture answers its stored values.
//!
//! # Orientation
//!
//! The rows of a render texture on the GPU are bottom first: [`super::camera::clip_from_gl`] keeps
//! the OpenGL clip Y as it is, which puts the lowest part of what is drawn in row 0 of the
//! texture, and the blit's coordinates run the same way. The world counts the rows of a read
//! bottom first too, so the rows of the copy go out in the order they came, with no flip. The
//! test `a_read_of_a_rectangle_answers_the_texels_a_blit_wrote_bottom_row_first` measures it.
//!
//! # What is refused
//!
//! A read is left unanswered, and counted by its reason in the report, when the texture holds
//! integers, is not on the GPU, cannot be copied (the budget has no room for the copy) or is in a
//! format whose texels this module cannot give as floats. The managed `ReadPixels` then keeps the
//! texels it had.

use worldrunner_render_program::runtime::PendingTargetRead;
use worldrunner_render_program::texels::rect_rgba;
use worldrunner_world_simulation::graphics::{OffscreenWork, RenderTextureId};

/// The texture holds integers, whose texels are not floats the world's read takes.
pub const REFUSED_INTEGER: &str = "integer format";
/// The texture the read names is not in the snapshot or has no storage on the GPU.
pub const REFUSED_NO_TEXTURE: &str = "no such render texture on the GPU";
/// The copy of the texture could not be recorded, because the storage budget has no room.
pub const REFUSED_NO_COPY: &str = "no copy of the texture could be made";
/// The copy could not be read back from the GPU.
pub const REFUSED_NOT_READ: &str = "the copy could not be read";
/// The texture's format is one this host does not give as floats.
pub const REFUSED_NOT_DECODED: &str = "format not given as floats";

/// The texels a flush answers a request with, bottom row first.
#[derive(Clone, Debug, PartialEq)]
pub struct PixelAnswer {
    pub request: u64,
    pub rgba: Vec<f32>,
}

/// A read the world asked for: the request, the render texture and the rectangle
/// `[x, y, width, height]` counted from the bottom left texel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Wanted {
    pub request: u64,
    pub texture: RenderTextureId,
    pub rect: [u32; 4],
}

impl Wanted {
    /// The read `work` records, if it is one.
    pub(super) fn of(work: &OffscreenWork) -> Option<Self> {
        let &OffscreenWork::ReadPixels {
            request,
            texture,
            x,
            y,
            width,
            height,
        } = work
        else {
            return None;
        };
        Some(Self {
            request,
            texture,
            rect: [x, y, width, height],
        })
    }
}

/// Whether a `GraphicsFormat` code names a format of integer texels (unsigned or signed).
///
/// The codes are the managed engine's own enumeration in `csharp/Disunity/Enums/Other.cs`: the
/// 8, 16 and 32-bit integers (13 to 20, 29 to 44), the blue-first 8-bit integers (62 to 65) and
/// the packed 10-bit integers (76, 77, 79 and 80).
#[must_use]
pub fn is_integer_format(code: u32) -> bool {
    matches!(code, 13..=20 | 29..=44 | 62..=65 | 76 | 77 | 79 | 80)
}

/// A copy of a texture recorded into a frame, to be read when the frame is submitted.
pub(super) struct Copied {
    wanted: Wanted,
    copy: PendingTargetRead,
}

impl Copied {
    pub(super) fn new(wanted: Wanted, copy: PendingTargetRead) -> Self {
        Self { wanted, copy }
    }

    /// Waits for the submission, reads the copy and gives the rectangle's texels.
    pub(super) fn resolve(
        self,
        submission: &wgpu::SubmissionIndex,
    ) -> Result<PixelAnswer, &'static str> {
        let Self { wanted, copy } = self;
        let image = copy
            .resolve(submission.clone())
            .map_err(|_| REFUSED_NOT_READ)?;
        let rgba =
            rect_rgba(&image.bytes, image.format, image.size, wanted.rect).ok_or_else(|| {
                if is_integer_texture(image.format) {
                    REFUSED_INTEGER
                } else {
                    REFUSED_NOT_DECODED
                }
            })?;
        Ok(PixelAnswer {
            request: wanted.request,
            rgba,
        })
    }
}

fn is_integer_texture(format: wgpu::TextureFormat) -> bool {
    matches!(
        format.sample_type(None, None),
        Some(wgpu::TextureSampleType::Uint | wgpu::TextureSampleType::Sint)
    )
}
