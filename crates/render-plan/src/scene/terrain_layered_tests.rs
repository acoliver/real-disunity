use super::*;

fn layer() -> TerrainLayer {
    TerrainLayer {
        albedo_metallic: TextureHandle::from_raw(0),
        normal_roughness: TextureHandle::from_raw(1),
        normal_encoding: SceneNormalEncoding::TwoChannelXy,
        height: TextureHandle::from_raw(2),
        repeat_millimeters: 1000,
        offset_millimeters: [0; 2],
        roughness_metallic_offset_micro: [0; 2],
    }
}

fn terrain(appearance: SceneTerrainAppearance, surfaces: usize) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(MaterialHandle::from_raw(0), appearance); surfaces],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn source() -> TerrainLayeredSource {
    TerrainLayeredSource {
        channel_map: None,
        type_grid: TerrainTypeGrid::new([2, 2], [0; 2], 1000, vec![0; 4]).unwrap(),
        filter: TerrainSplatFilter::Spiral14,
        colorization: None,
    }
}

fn explicit_textures() -> Vec<SceneTexture> {
    [
        TexelFormat::Rgba8Unorm,
        TexelFormat::Rgba8Unorm,
        TexelFormat::R8Unorm,
    ]
    .into_iter()
    .map(|format| SceneTexture::Texels {
        format,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128; if format == TexelFormat::R8Unorm { 1 } else { 4 }].into_boxed_slice(),
    })
    .collect()
}

#[test]
fn layered_inputs_reject_encoded_layouts_in_every_slot() {
    let terrain = terrain(
        SceneTerrainAppearance {
            layered: Some(layer()),
            ..SceneTerrainAppearance::NEUTRAL
        },
        1,
    )
    .with_layered_source(source())
    .unwrap();
    let encoded = SceneTexture::Encoded {
        encoding: TextureEncoding::OpaqueExact,
        color_space: TextureColorSpace::Linear,
        bytes: Box::default(),
    };
    for slot in 0..3 {
        for wrapped in [false, true] {
            let mut textures = explicit_textures();
            textures[slot] = if wrapped {
                encoded.clone().with_sampler(TextureSampler::default())
            } else {
                encoded.clone()
            };
            assert!(source().validate_resources(&terrain, &textures).is_err());
        }
    }
    for channels in [
        TerrainColorizationChannels::Rgb,
        TerrainColorizationChannels::Grayscale,
        TerrainColorizationChannels::BgrRoughness,
    ] {
        let source = TerrainLayeredSource {
            channel_map: None,
            colorization: Some(TerrainColorization {
                texture: TextureHandle::from_raw(3),
                channels,
                origin_millimeters: [0; 2],
                extent_millimeters: [1000; 2],
            }),
            ..source()
        };
        let mut textures = explicit_textures();
        textures.push(encoded.clone().with_sampler(TextureSampler::default()));
        assert!(source.validate_resources(&terrain, &textures).is_err());
    }
}

#[test]
fn scene_admission_rejects_orphan_layers_with_invalid_handles() {
    let terrain = terrain(
        SceneTerrainAppearance {
            layered: Some(layer()),
            ..SceneTerrainAppearance::NEUTRAL
        },
        1,
    );
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let result = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![explicit_textures().remove(0)], vec![material])
            .unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: Some(terrain),
        effects: None,
    });
    assert!(matches!(
        result,
        Err(SceneValidationError::UnsupportedInput {
            part: "terrain layers require a layered source",
        })
    ));
}

#[test]
fn layered_sources_require_every_surface_to_declare_a_layer() {
    assert!(
        terrain(SceneTerrainAppearance::NEUTRAL, 1)
            .with_layered_source(source())
            .is_err()
    );
    let appearance = SceneTerrainAppearance {
        layered: Some(layer()),
        ..SceneTerrainAppearance::NEUTRAL
    };
    assert!(
        terrain(appearance, 256)
            .with_layered_source(source())
            .is_ok()
    );
    assert!(
        terrain(appearance, 257)
            .with_layered_source(source())
            .is_err()
    );
}

#[test]
fn texture_closure_includes_all_layer_and_tint_handles() {
    let appearance = SceneTerrainAppearance {
        layered: Some(layer()),
        ..SceneTerrainAppearance::NEUTRAL
    };
    let terrain = terrain(appearance, 1)
        .with_layered_source(source())
        .unwrap();
    assert!(source().validate_handles(&terrain, 2).is_err());
    assert!(source().validate_handles(&terrain, 3).is_ok());
    let tinted = TerrainLayeredSource {
        channel_map: None,
        colorization: Some(TerrainColorization {
            texture: TextureHandle::from_raw(3),
            channels: TerrainColorizationChannels::Grayscale,
            origin_millimeters: [0; 2],
            extent_millimeters: [1000; 2],
        }),
        ..source()
    };
    assert!(tinted.validate_handles(&terrain, 3).is_err());
    assert!(tinted.validate_handles(&terrain, 4).is_ok());
}

#[test]
fn invalid_layer_parameters_fail_during_terrain_adoption() {
    let invalid = TerrainLayer {
        repeat_millimeters: 0,
        ..layer()
    };
    assert!(invalid.validate().is_err());
    let invalid = TerrainLayer {
        roughness_metallic_offset_micro: [1_000_001, 0],
        ..layer()
    };
    assert!(invalid.validate().is_err());
    let invalid = TerrainLayer {
        roughness_metallic_offset_micro: [i32::MIN, 0],
        ..layer()
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn layered_data_rejects_srgb_and_wrong_channel_layouts() {
    let appearance = SceneTerrainAppearance {
        layered: Some(layer()),
        ..SceneTerrainAppearance::NEUTRAL
    };
    let terrain = terrain(appearance, 1)
        .with_layered_source(source())
        .unwrap();
    let texture = |format, color_space, bytes: Vec<u8>| SceneTexture::Texels {
        format,
        color_space,
        width: 1,
        height: 1,
        mips: 1,
        bytes: bytes.into_boxed_slice(),
    };
    let mut textures = vec![
        texture(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Srgb,
            vec![128; 4],
        ),
        texture(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Linear,
            vec![128; 4],
        ),
        texture(TexelFormat::R8Unorm, TextureColorSpace::Linear, vec![128]),
    ];
    assert!(source().validate_resources(&terrain, &textures).is_ok());
    textures[1] = texture(
        TexelFormat::Rgba8Unorm,
        TextureColorSpace::Srgb,
        vec![128; 4],
    );
    assert!(source().validate_resources(&terrain, &textures).is_err());
    textures[1] = texture(TexelFormat::R8Unorm, TextureColorSpace::Linear, vec![128]);
    assert!(source().validate_resources(&terrain, &textures).is_err());
}

#[test]
fn independent_type_grid_validates_dimensions_payload_and_palette() {
    assert!(TerrainTypeGrid::new([0, 1], [0; 2], 1, vec![]).is_err());
    assert!(TerrainTypeGrid::new([2, 1], [0; 2], 1, vec![0]).is_err());
    assert!(TerrainTypeGrid::new([1, 1], [0; 2], 0, vec![0]).is_err());
    let appearance = SceneTerrainAppearance {
        layered: Some(layer()),
        ..SceneTerrainAppearance::NEUTRAL
    };
    let terrain = terrain(appearance, 1);
    let source = TerrainLayeredSource {
        channel_map: None,
        type_grid: TerrainTypeGrid::new([1, 1], [0; 2], 1, vec![1]).unwrap(),
        ..source()
    };
    assert!(terrain.with_layered_source(source).is_err());
}

fn compressed_colorization(format: u32) -> SceneTexture {
    let mut bytes = vec![0; 148 + if matches!(format, 71 | 80) { 8 } else { 16 }];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[4..8].copy_from_slice(&124_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&4_u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&4_u32.to_le_bytes());
    bytes[28..32].copy_from_slice(&1_u32.to_le_bytes());
    bytes[84..88].copy_from_slice(b"DX10");
    bytes[128..132].copy_from_slice(&format.to_le_bytes());
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bytes.into_boxed_slice(),
    }
}

#[test]
fn compressed_colorization_validates_consumed_components() {
    let terrain = terrain(
        SceneTerrainAppearance {
            layered: Some(layer()),
            ..SceneTerrainAppearance::NEUTRAL
        },
        1,
    );
    for format in [71, 77, 80, 83] {
        for channels in [
            TerrainColorizationChannels::Grayscale,
            TerrainColorizationChannels::Rgb,
            TerrainColorizationChannels::BgrRoughness,
        ] {
            let source = TerrainLayeredSource {
                colorization: Some(TerrainColorization {
                    texture: TextureHandle::from_raw(3),
                    channels,
                    origin_millimeters: [0; 2],
                    extent_millimeters: [1000; 2],
                }),
                ..source()
            };
            for wrapped in [false, true] {
                let mut textures = explicit_textures();
                let texture = compressed_colorization(format);
                textures.push(if wrapped {
                    texture.with_sampler(TextureSampler::default())
                } else {
                    texture
                });
                let valid = channels == TerrainColorizationChannels::Grayscale || format < 80;
                assert_eq!(
                    source.validate_resources(&terrain, &textures).is_ok(),
                    valid,
                    "format {format}, channels {channels:?}, wrapped {wrapped}"
                );
            }
        }
    }
}
