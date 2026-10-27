use super::{OffscreenError, Viewport};

/// Linear scene colour before exposure, tone mapping and display conversion.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearRgbaImage {
    pub viewport: Viewport,
    pub pixels: Vec<[f32; 4]>,
}
impl LinearRgbaImage {
    /// Decodes tightly packed native little-endian RGBA floating-point texels.
    pub fn from_native(
        viewport: Viewport,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<Self, OffscreenError> {
        let stride = match format {
            wgpu::TextureFormat::Rgba16Float => 8,
            wgpu::TextureFormat::Rgba32Float => 16,
            _ => {
                return Err(OffscreenError::Output(
                    worldrunner_render_post::ReadbackError::Format(format),
                ));
            }
        };
        let count = u64::from(viewport.width)
            .checked_mul(u64::from(viewport.height))
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(OffscreenError::LayoutOverflow)?;
        let expected = count
            .checked_mul(stride)
            .ok_or(OffscreenError::LayoutOverflow)?;
        if bytes.len() != expected {
            return Err(OffscreenError::ShortReadback {
                expected,
                actual: bytes.len(),
            });
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(OffscreenError::Allocation)?;
        for pixel in bytes.chunks_exact(stride) {
            pixels.push(std::array::from_fn(|c| {
                if stride == 8 {
                    half::f16::from_le_bytes([pixel[c * 2], pixel[c * 2 + 1]]).to_f32()
                } else {
                    f32::from_le_bytes(pixel[c * 4..c * 4 + 4].try_into().expect("RGBA32 channel"))
                }
            }));
        }
        Ok(Self { viewport, pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn float_formats_preserve_bright_negative_and_alpha_values() {
        let viewport = Viewport {
            width: 1,
            height: 1,
        };
        let expected = [4.0, -0.5, 0.25, 1.0];
        let half: Vec<_> = expected
            .iter()
            .flat_map(|v| half::f16::from_f32(*v).to_le_bytes())
            .collect();
        let full: Vec<_> = expected.iter().flat_map(|v| v.to_le_bytes()).collect();
        for (format, bytes) in [
            (wgpu::TextureFormat::Rgba16Float, half),
            (wgpu::TextureFormat::Rgba32Float, full),
        ] {
            assert_eq!(
                LinearRgbaImage::from_native(viewport, format, &bytes)
                    .unwrap()
                    .pixels,
                vec![expected]
            );
            assert!(
                LinearRgbaImage::from_native(viewport, format, &bytes[..bytes.len() - 1]).is_err()
            );
        }
    }
}
