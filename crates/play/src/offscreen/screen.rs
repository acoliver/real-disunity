//! The screen: the colour texture a command buffer's `CameraTarget` draws into when no camera
//! texture names it.
//!
//! The host keeps the screen's texture, a colour with a depth buffer, like a render texture of the
//! world: it persists from frame to frame until the screen's size changes, so a draw with no clear
//! keeps the colour and the depth an earlier frame left. A run writes its screens from the texture
//! itself (the viewer composes it with the UI on the GPU), so the host only notes
//! that work drew into the colour ([`writes_screen`]).
//!
//! Tests read what draws left in the screen back as a `ScreenImage`, with the copy and read that
//! answers a script's `ReadPixels` (see the `pixels` module). The rows of the colour texture on the
//! GPU are bottom first, as the `pixels` module documents; a `ScreenImage` holds its rows top
//! first, the order an image is stored and shown in, so the rows of the copy are turned over. The
//! read is refused, with its reason, when no screen is set or the colour has no storage yet, the
//! budget has no room for the copy, the copy cannot be read or the texture is not four 8-bit
//! channels.

#[cfg(test)]
use worldrunner_render_program::runtime::TargetImage;
use worldrunner_render_program::unity::offscreen::frame::{Attachments, Record};

use super::ids;

/// The screen's colour is not four 8-bit channels, the only form of a [`ScreenImage`].
#[cfg(test)]
pub const REFUSED_SCREEN_FORMAT: &str = "the screen is not four 8-bit channels";

/// What the screen held after a frame's work: 8-bit RGBA texels as the sRGB colour texture stores
/// them, rows top first.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenImage {
    pub width: u32,
    pub height: u32,
    /// `width * height` texels of red, green, blue and alpha, the top row first.
    pub rgba: Vec<u8>,
    /// The number of the frame whose work (or a flush before it) drew this, counting from zero.
    pub frame: u64,
}

#[cfg(test)]
impl ScreenImage {
    /// The image of a copy of the screen's colour, whose rows are bottom first.
    ///
    /// # Errors
    /// Fails with the reason when the copy is not four 8-bit channels or does not hold its rows.
    pub(super) fn from_gpu_rows(image: &TargetImage, frame: u64) -> Result<Self, &'static str> {
        let [width, height] = image.size;
        let eight_bit = matches!(
            image.format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba8Unorm
        );
        let row = width as usize * 4;
        if !eight_bit || row == 0 || image.bytes.len() != row * height as usize {
            return Err(REFUSED_SCREEN_FORMAT);
        }
        Ok(Self {
            width,
            height,
            rgba: image
                .bytes
                .chunks_exact(row)
                .rev()
                .flatten()
                .copied()
                .collect(),
            frame,
        })
    }

    /// The texel at column `x`, row `y` counted from the top.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        std::array::from_fn(|channel| self.rgba[at + channel])
    }
}

/// Whether any of `records` writes the screen's colour: a clear of the colour, a draw whose colour
/// target is the screen, or a render, blit or copy into it. A record that only uses the screen's
/// depth buffer, or reads the screen, leaves its colour as it was.
pub(super) fn writes_screen(records: &[Record]) -> bool {
    records.iter().any(writes_screen_colour)
}

fn writes_screen_colour(record: &Record) -> bool {
    let screen = ids::screen_color();
    let colour_is_screen =
        |target: &Attachments| target.colors.iter().any(|colour| colour.texture == screen);
    match record {
        Record::CameraRender { target, .. }
        | Record::Blit { target, .. }
        | Record::CopyTexture { target, .. } => *target == screen,
        Record::Clear { target, clear } => clear.color.is_some() && colour_is_screen(target),
        Record::Draws(list) => colour_is_screen(&list.target),
        Record::Procedural(draw) => colour_is_screen(&draw.target),
        // Generating mips writes the levels below the base, and the screen has only its base.
        Record::Globals(_) | Record::GenerateMips { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A copy of `rows` rows of two texels, texel `n` of the copy holding `[n, n, n, 255]`.
    fn copy(rows: u32, format: wgpu::TextureFormat) -> TargetImage {
        let texels = (0..rows * 2).flat_map(|n| {
            let n = u8::try_from(n).unwrap();
            [n, n, n, 255]
        });
        TargetImage {
            size: [2, rows],
            format,
            bytes: texels.collect(),
        }
    }

    #[test]
    fn the_rows_of_a_copy_are_turned_over_so_the_image_starts_at_the_top() {
        // The copy's first row is the bottom of the screen; its texels are 0 and 1.
        let image =
            ScreenImage::from_gpu_rows(&copy(3, wgpu::TextureFormat::Rgba8UnormSrgb), 7).unwrap();
        assert_eq!((image.width, image.height, image.frame), (2, 3, 7));
        assert_eq!(
            image.pixel(0, 0),
            [4, 4, 4, 255],
            "the top row is the copy's last"
        );
        assert_eq!(image.pixel(1, 0), [5, 5, 5, 255]);
        assert_eq!(image.pixel(0, 1), [2, 2, 2, 255]);
        assert_eq!(
            image.pixel(0, 2),
            [0, 0, 0, 255],
            "the bottom row is the copy's first"
        );
        assert_eq!(image.pixel(1, 2), [1, 1, 1, 255]);
    }

    #[test]
    fn a_format_that_is_not_four_eight_bit_channels_is_refused() {
        for format in [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::R8Unorm,
        ] {
            assert_eq!(
                ScreenImage::from_gpu_rows(&copy(2, format), 0),
                Err(REFUSED_SCREEN_FORMAT)
            );
        }
    }

    #[test]
    fn a_copy_that_does_not_hold_its_rows_is_refused() {
        let mut short = copy(2, wgpu::TextureFormat::Rgba8UnormSrgb);
        short.bytes.pop();
        assert_eq!(
            ScreenImage::from_gpu_rows(&short, 0),
            Err(REFUSED_SCREEN_FORMAT)
        );
    }
}
