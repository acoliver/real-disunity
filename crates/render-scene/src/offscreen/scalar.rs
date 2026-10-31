use crate::Viewport;

/// One native scalar per texel, in top-to-bottom row order, without color conversion.
#[derive(Clone, Debug, PartialEq)]
pub struct ScalarImage {
    pub viewport: Viewport,
    pub pixels: Vec<f32>,
}
impl ScalarImage {
    /// Decodes `R32Float`, `Rg16Float`/`Rg32Float` red, or `Rgba16Float` alpha.
    pub fn from_native(
        viewport: Viewport,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<Self, &'static str> {
        let stride = match format {
            wgpu::TextureFormat::R32Float | wgpu::TextureFormat::Rg16Float => 4,
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rg32Float => 8,
            _ => return Err("unsupported scalar image format"),
        };
        let count = u64::from(viewport.width) * u64::from(viewport.height);
        if u64::try_from(bytes.len()).expect("byte length") != count * stride {
            return Err("scalar image byte length");
        }
        let pixels = bytes
            .chunks_exact(usize::try_from(stride).expect("stride"))
            .map(|p| match format {
                wgpu::TextureFormat::R32Float | wgpu::TextureFormat::Rg32Float => {
                    f32::from_le_bytes(p[..4].try_into().expect("texel"))
                }
                wgpu::TextureFormat::Rg16Float => half::f16::from_le_bytes([p[0], p[1]]).to_f32(),
                wgpu::TextureFormat::Rgba16Float => half::f16::from_le_bytes([p[6], p[7]]).to_f32(),
                _ => unreachable!("validated scalar format"),
            })
            .collect();
        Ok(Self { viewport, pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_formats_decode_selected_native_channel_and_reject_bad_lengths() {
        let viewport = Viewport {
            width: 1,
            height: 1,
        };
        for (format, bytes, expected) in [
            (
                wgpu::TextureFormat::R32Float,
                12.5_f32.to_le_bytes().to_vec(),
                12.5,
            ),
            (
                wgpu::TextureFormat::Rg32Float,
                [0.25_f32, 123.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect(),
                0.25,
            ),
            (wgpu::TextureFormat::Rg16Float, vec![0, 56, 0, 60], 0.5),
            (
                wgpu::TextureFormat::Rgba16Float,
                vec![0, 0, 0, 60, 0, 0, 0, 52],
                0.25,
            ),
        ] {
            assert_eq!(
                ScalarImage::from_native(viewport, format, &bytes)
                    .unwrap()
                    .pixels,
                [expected]
            );
            assert!(ScalarImage::from_native(viewport, format, &bytes[..bytes.len() - 1]).is_err());
        }
    }
}

#[cfg(test)]
#[test]
fn scalar_rg32_decodes_red_with_eight_byte_stride() {
    let bytes: Vec<_> = [0.25_f32, 123.0, 0.75, 456.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let image = ScalarImage::from_native(
        Viewport {
            width: 2,
            height: 1,
        },
        wgpu::TextureFormat::Rg32Float,
        &bytes,
    )
    .unwrap();
    assert_eq!(image.pixels, [0.25, 0.75]);
}
