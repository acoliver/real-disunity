use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureAnisotropy, TextureColorSpace};

#[test]
fn material_descriptors_only_change_anisotropy() {
    for level in [1, 2, 4, 8, 16] {
        let actual =
            crate::upload::material_sampler_descriptor(TextureAnisotropy::new(level).unwrap());
        assert_eq!(actual.anisotropy_clamp, level);
        assert_eq!(actual.mag_filter, wgpu::FilterMode::Linear);
        assert_eq!(actual.min_filter, wgpu::FilterMode::Linear);
        assert_eq!(actual.mipmap_filter, wgpu::MipmapFilterMode::Linear);
        assert_eq!(actual.address_mode_u, wgpu::AddressMode::Repeat);
        assert_eq!(actual.address_mode_v, wgpu::AddressMode::Repeat);
        assert_eq!(actual.address_mode_w, wgpu::AddressMode::Repeat);
    }
}

pub(crate) fn checker() -> SceneTexture {
    let mut bytes = Vec::new();
    for mip in 0..9 {
        let size = 256_u32 >> mip;
        for y in 0..size {
            for x in 0..size {
                let value = if mip >= 3 {
                    128
                } else if ((x << mip) / 4 + (y << mip) / 64) % 2 == 0 {
                    255
                } else {
                    0
                };
                bytes.extend([value, value, value, 255]);
            }
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 256,
        height: 256,
        mips: 9,
        bytes: bytes.into(),
    }
}
