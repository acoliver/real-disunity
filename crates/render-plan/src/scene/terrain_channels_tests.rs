#[test]
fn layered_channels_reject_half_float_formats() {
    for format in [TexelFormat::Rg16Float, TexelFormat::Rgba16Float] {
        for slot in 0..3 {
            for sampled in [false, true] {
                let mut textures = textures();
                let texture = SceneTexture::Texels {
                    format,
                    color_space: if slot == 0 {
                        TextureColorSpace::Srgb
                    } else {
                        TextureColorSpace::Linear
                    },
                    width: 1,
                    height: 1,
                    mips: 1,
                    bytes: vec![0; usize::try_from(format.bytes_per_texel()).unwrap()].into(),
                };
                textures[slot] = if sampled {
                    texture.with_sampler(TextureSampler::default())
                } else {
                    texture
                };
                assert_eq!(
                    map().validate(layer(), &textures),
                    Err(SceneValidationError::UnsupportedInput {
                        part: "terrain layered texture format",
                    }),
                    "format={format:?}, slot={slot}, sampled={sampled}"
                );
            }
        }
    }
}

use super::*;

fn layer() -> TerrainLayer {
    TerrainLayer {
        albedo_metallic: TextureHandle::from_raw(0),
        normal_roughness: TextureHandle::from_raw(1),
        height: TextureHandle::from_raw(2),
        normal_encoding: SceneNormalEncoding::TwoChannelXy,
        repeat_millimeters: 1000,
        offset_millimeters: [0; 2],
        roughness_metallic_offset_micro: [0; 2],
    }
}

fn textures() -> Vec<SceneTexture> {
    [
        (TexelFormat::Rgba8Unorm, TextureColorSpace::Srgb),
        (TexelFormat::Rg8Unorm, TextureColorSpace::Linear),
        (TexelFormat::R8Unorm, TextureColorSpace::Linear),
    ]
    .into_iter()
    .map(|(format, color_space)| SceneTexture::Texels {
        format,
        color_space,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128; usize::try_from(format.bytes_per_texel()).unwrap()].into_boxed_slice(),
    })
    .collect()
}

fn map() -> TerrainLayerChannelMap {
    let mut map =
        TerrainLayerChannelMap::packed(SceneNormalEncoding::TwoChannelXy, TextureColorSpace::Srgb);
    map.roughness = TerrainLayerChannel {
        texture: TerrainLayerTexture::Height,
        channel: 0,
    };
    map
}

#[test]
fn channel_map_validates_each_used_component_and_linear_srgb_alpha() {
    let textures = textures();
    let valid = map();
    assert!(valid.validate(layer(), &textures).is_ok());
    let mut invalid = valid;
    invalid.albedo[0].channel = 4;
    assert!(invalid.validate(layer(), &textures).is_err());
    invalid = valid;
    invalid.normal[1].channel = 2;
    assert!(invalid.validate(layer(), &textures).is_err());
    invalid = valid;
    invalid.normal_encoding = SceneNormalEncoding::Rgb;
    assert!(invalid.validate(layer(), &textures).is_err());
    invalid = valid;
    invalid.height.channel = 1;
    assert!(invalid.validate(layer(), &textures).is_err());
    invalid = valid;
    invalid.albedo_color_space = TextureColorSpace::Linear;
    assert!(invalid.validate(layer(), &textures).is_err());
    invalid = valid;
    invalid.metallic.channel = 0;
    assert!(invalid.validate(layer(), &textures).is_err());
    let missing = TerrainLayer {
        height: TextureHandle::from_raw(10),
        ..layer()
    };
    assert!(matches!(
        valid.validate(missing, &textures),
        Err(SceneValidationError::HandleOutOfRange { handle: 10, .. })
    ));
}

#[test]
fn dds_channel_map_rejects_container_color_space_disagreement() {
    let mut textures = textures();
    let mut bytes = vec![0; 148];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[84..88].copy_from_slice(b"DX10");
    bytes[128..132].copy_from_slice(&77_u32.to_le_bytes());
    textures[0] = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Srgb,
        bytes: bytes.into_boxed_slice(),
    };
    assert!(matches!(
        map().validate(layer(), &textures),
        Err(SceneValidationError::UnsupportedInput {
            part: "terrain DDS color space mismatch"
        })
    ));
}
