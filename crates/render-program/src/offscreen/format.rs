//! The `GraphicsFormat` values offscreen work draws into and samples, and the depth formats.
//! Only formats a test draws with are in the table; any other is a named error.

use super::error::OffscreenError;

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
