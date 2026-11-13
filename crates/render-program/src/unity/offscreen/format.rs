//! The `GraphicsFormat` values offscreen work draws into and samples, and the depth formats.
//! Only formats a test draws with are in the table; any other is a named error.

use super::error::OffscreenError;

/// The `GraphicsFormat` codes offscreen work draws into and samples: every code of the
/// [`color_format`] table. A player reports these as the render formats its device supports.
pub const RENDER_FORMATS: [u32; 6] = [4, 5, 8, 45, 48, 52];

/// The wgpu format of an engine `GraphicsFormat` code.
pub(crate) fn color_format(code: u32) -> Result<wgpu::TextureFormat, OffscreenError> {
    Ok(match code {
        4 => wgpu::TextureFormat::Rgba8UnormSrgb,
        5 => wgpu::TextureFormat::R8Unorm,
        8 => wgpu::TextureFormat::Rgba8Unorm,
        45 => wgpu::TextureFormat::R16Float,
        48 => wgpu::TextureFormat::Rgba16Float,
        52 => wgpu::TextureFormat::Rgba32Float,
        _ => return Err(OffscreenError::UnsupportedFormat { code }),
    })
}

/// The wgpu format of an uploaded source texture: a render format, or a block-compressed format
/// the player stores and the device samples as it is. BC6H (106) is unsigned-float, the form the
/// managed engine's texture format 24 maps to. DXT1 (96 sRGB, 97 linear), DXT5 (100, 101), BC4
/// (102, one unsigned-normalised channel) and BC7 (108, 109) are the other block formats.
pub(crate) fn source_format(code: u32) -> Result<wgpu::TextureFormat, OffscreenError> {
    use wgpu::TextureFormat as F;
    match code {
        96 => Ok(F::Bc1RgbaUnormSrgb),
        97 => Ok(F::Bc1RgbaUnorm),
        100 => Ok(F::Bc3RgbaUnormSrgb),
        101 => Ok(F::Bc3RgbaUnorm),
        102 => Ok(F::Bc4RUnorm),
        106 => Ok(F::Bc6hRgbUfloat),
        108 => Ok(F::Bc7RgbaUnormSrgb),
        109 => Ok(F::Bc7RgbaUnorm),
        _ => color_format(code),
    }
}

/// The color format of a render texture: `None` for `GraphicsFormat` 0, a target with only depth,
/// else the format of the table.
pub(crate) fn render_color_format(
    code: u32,
) -> Result<Option<wgpu::TextureFormat>, OffscreenError> {
    if code == 0 {
        return Ok(None);
    }
    color_format(code).map(Some)
}

/// The depth format of `bits` depth bits, or `None` for none.
pub(crate) fn depth_format(bits: u8) -> Result<Option<wgpu::TextureFormat>, OffscreenError> {
    Ok(match bits {
        0 => None,
        16 => Some(wgpu::TextureFormat::Depth16Unorm),
        24 => Some(wgpu::TextureFormat::Depth24Plus),
        32 => Some(wgpu::TextureFormat::Depth32Float),
        _ => return Err(OffscreenError::UnsupportedDepth { bits }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_render_formats_are_exactly_the_color_table() {
        let mapped: Vec<u32> = (0..256)
            .filter(|&code| color_format(code).is_ok())
            .collect();
        assert_eq!(mapped, RENDER_FORMATS);
    }

    #[test]
    fn the_formats_with_a_test_map_to_their_wgpu_formats() {
        for (code, format) in [
            (4, wgpu::TextureFormat::Rgba8UnormSrgb),
            (5, wgpu::TextureFormat::R8Unorm),
            (8, wgpu::TextureFormat::Rgba8Unorm),
            (45, wgpu::TextureFormat::R16Float),
            (48, wgpu::TextureFormat::Rgba16Float),
            (52, wgpu::TextureFormat::Rgba32Float),
        ] {
            assert_eq!(color_format(code).unwrap(), format);
        }
    }

    #[test]
    fn a_render_texture_may_have_no_color_format_and_every_other_code_is_the_sampled_table() {
        assert_eq!(render_color_format(0).unwrap(), None);
        assert_eq!(
            render_color_format(45).unwrap(),
            Some(wgpu::TextureFormat::R16Float)
        );
        assert_eq!(
            render_color_format(52).unwrap(),
            Some(wgpu::TextureFormat::Rgba32Float)
        );
        assert!(matches!(
            render_color_format(93),
            Err(OffscreenError::UnsupportedFormat { code: 93 })
        ));
    }

    #[test]
    fn a_source_may_be_bc6h_and_a_render_texture_may_not() {
        assert_eq!(
            source_format(106).unwrap(),
            wgpu::TextureFormat::Bc6hRgbUfloat
        );
        assert_eq!(source_format(8).unwrap(), wgpu::TextureFormat::Rgba8Unorm);
        assert!(matches!(
            render_color_format(106),
            Err(OffscreenError::UnsupportedFormat { code: 106 })
        ));
    }

    #[test]
    fn a_source_may_be_any_block_format_the_player_stores() {
        use wgpu::TextureFormat as F;
        for (code, format) in [
            (96, F::Bc1RgbaUnormSrgb),
            (97, F::Bc1RgbaUnorm),
            (100, F::Bc3RgbaUnormSrgb),
            (101, F::Bc3RgbaUnorm),
            (102, F::Bc4RUnorm),
            (106, F::Bc6hRgbUfloat),
            (108, F::Bc7RgbaUnormSrgb),
            (109, F::Bc7RgbaUnorm),
        ] {
            assert_eq!(source_format(code).unwrap(), format, "code {code}");
            assert!(
                render_color_format(code).is_err(),
                "a block format is never a render target (code {code})"
            );
        }
    }

    #[test]
    fn other_formats_are_named_errors_with_their_code() {
        // Rgb8 has no wgpu format; 93 is a depth format, which is not a color format.
        for code in [0, 7, 49, 93, 101, 4000] {
            assert!(matches!(
                color_format(code),
                Err(OffscreenError::UnsupportedFormat { code: found }) if found == code
            ));
        }
    }

    #[test]
    fn depth_comes_from_depth_bits() {
        assert_eq!(depth_format(0).unwrap(), None);
        assert_eq!(
            depth_format(16).unwrap(),
            Some(wgpu::TextureFormat::Depth16Unorm)
        );
        assert_eq!(
            depth_format(24).unwrap(),
            Some(wgpu::TextureFormat::Depth24Plus)
        );
        assert_eq!(
            depth_format(32).unwrap(),
            Some(wgpu::TextureFormat::Depth32Float)
        );
        assert!(matches!(
            depth_format(8),
            Err(OffscreenError::UnsupportedDepth { bits: 8 })
        ));
    }
}
