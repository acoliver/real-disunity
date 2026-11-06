use worldrunner_render_plan::{
    SceneAlphaMode, SceneImpostor, SceneImpostorLayout, SceneMaterial, SceneMaterialChannels,
    SceneMaterialParts, SceneResources, SceneTexture, SceneValidationError, TexelFormat,
    TextureChannel, TextureColorSpace, TextureHandle,
};

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: Some(TextureHandle::from_raw(1)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 1_000_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

fn texels(format: TexelFormat) -> SceneTexture {
    SceneTexture::Texels {
        format,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0; usize::try_from(format.bytes_per_texel()).unwrap()].into(),
    }
}

#[test]
fn defaults_preserve_surface_and_packed_impostor_layouts() {
    use TextureChannel::{B, G, R};
    let channels = material().channels();
    assert_eq!(channels, SceneMaterialChannels::default());
    assert_eq!((channels.roughness, channels.metalness), (G, B));
    assert_eq!(
        (channels.ambient_occlusion, channels.self_illumination),
        (R, R)
    );
    let atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    let packed = atlas.material_channels();
    assert_eq!((packed.roughness, packed.metalness), (R, G));
    assert_eq!((packed.ambient_occlusion, packed.self_illumination), (G, R));
}

#[test]
fn invalid_indices_fail_and_shared_channels_are_admitted() {
    assert_eq!(
        TextureChannel::try_from(4),
        Err(SceneValidationError::InvalidTextureChannel { observed: 4 })
    );
    for channel in [
        TextureChannel::R,
        TextureChannel::G,
        TextureChannel::B,
        TextureChannel::A,
    ] {
        assert!(
            material()
                .with_channels(SceneMaterialChannels {
                    roughness: channel,
                    metalness: channel,
                    ..SceneMaterialChannels::default()
                })
                .is_ok()
        );
    }
    let atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    assert!(
        atlas
            .with_material_channels(SceneMaterialChannels {
                roughness: TextureChannel::R,
                metalness: TextureChannel::G,
                ambient_occlusion: TextureChannel::B,
                self_illumination: TextureChannel::B,
            })
            .is_ok()
    );
}

#[test]
fn resource_admission_rejects_channels_absent_from_raw_formats() {
    for format in [
        TexelFormat::R8Unorm,
        TexelFormat::Rg8Unorm,
        TexelFormat::Rg16Float,
    ] {
        assert!(matches!(
            SceneResources::new(
                vec![texels(TexelFormat::Rgba8Unorm), texels(format)],
                vec![material()]
            ),
            Err(SceneValidationError::MissingTextureChannel { .. })
        ));
    }
    let rb = material()
        .with_channels(SceneMaterialChannels {
            roughness: TextureChannel::R,
            metalness: TextureChannel::G,
            ..SceneMaterialChannels::default()
        })
        .unwrap();
    assert!(
        SceneResources::new(
            vec![
                texels(TexelFormat::Rgba8Unorm),
                texels(TexelFormat::Rg8Unorm)
            ],
            vec![rb]
        )
        .is_ok()
    );
}

#[test]
fn r8_admits_shared_roughness_and_metalness() {
    let shared = material()
        .with_channels(SceneMaterialChannels {
            roughness: TextureChannel::R,
            metalness: TextureChannel::R,
            ..SceneMaterialChannels::default()
        })
        .unwrap();
    assert!(
        SceneResources::new(
            vec![
                texels(TexelFormat::Rgba8Unorm),
                texels(TexelFormat::R8Unorm)
            ],
            vec![shared]
        )
        .is_ok()
    );
}

fn scene(material: &SceneMaterial) -> worldrunner_render_plan::RenderScene {
    use worldrunner_render_plan::*;
    let vertices = [[0, 0, 0], [1000, 0, 0], [0, 1000, 0]].map(|position| SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: [0, 0, 1_000_000],
        texcoord_micro: [0, 0],
    });
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texels(TexelFormat::Rgba8Unorm); 2], vec![*material])
            .unwrap(),
        geometry: SceneGeometry::new(
            vec![
                SceneMesh::new(vec![
                    SceneMeshLod::new(0, vertices.to_vec(), vec![0, 1, 2], None).unwrap(),
                ])
                .unwrap(),
            ],
            vec![],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn ordinary_terrain_palettes_reject_custom_selectors_on_install_and_resource_replacement() {
    use std::sync::Arc;
    use worldrunner_render_plan::*;
    let terrain = Arc::new(
        SceneTerrain::new(SceneTerrainParts {
            columns: 2,
            rows: 2,
            origin_east_millimeters: 0,
            origin_north_millimeters: 0,
            sample_spacing_millimeters: 1000,
            heights_millimeters: vec![0; 4],
            surface_indices: vec![0; 4],
            surfaces: vec![SceneTerrainSurface::new(
                MaterialHandle::from_raw(0),
                SceneTerrainAppearance::NEUTRAL,
            )],
            lod_policy: TerrainLodPolicy::FullResolution,
        })
        .unwrap(),
    );
    let custom = material()
        .with_channels(SceneMaterialChannels {
            roughness: TextureChannel::R,
            ..SceneMaterialChannels::default()
        })
        .unwrap();
    let error = SceneValidationError::UnsupportedMaterialChannels {
        part: "terrain palette",
    };
    assert_eq!(
        scene(&custom)
            .with_terrain(Some(terrain.clone()))
            .unwrap_err(),
        error
    );
    let admitted = scene(&material()).with_terrain(Some(terrain)).unwrap();
    assert_eq!(
        admitted
            .with_resources(
                scene(&custom).resources().clone(),
                admitted.geometry().clone(),
                None
            )
            .unwrap_err(),
        error
    );
}

#[test]
fn decals_reject_custom_selectors_on_install_and_resource_replacement() {
    use worldrunner_render_plan::*;
    let decal = SceneDecal {
        priority: 0,
        transform: SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        dimensions_millimeters: [1000; 3],
        material: MaterialHandle::from_raw(0),
        axis: SceneDecalAxis::Z,
        edge_fade_micro: 0,
        angle_cutoff_micro: 0,
        angle_fade_micro: 0,
        receivers: SceneDecalReceivers::ALL,
        mapping: SceneDecalMapping::default(),
        color_modifier: SceneDecalColorModifier::default(),
        write_mask: SceneDecalWriteMask::ALL,
        alpha_test_cutoff_micro: None,
    };
    let custom = material()
        .with_channels(SceneMaterialChannels {
            roughness: TextureChannel::A,
            ..SceneMaterialChannels::default()
        })
        .unwrap();
    let error = SceneValidationError::UnsupportedMaterialChannels { part: "decal" };
    assert_eq!(scene(&custom).with_decals(vec![decal]).unwrap_err(), error);
    let admitted = scene(&material()).with_decals(vec![decal]).unwrap();
    assert_eq!(
        admitted
            .with_resources(
                scene(&custom).resources().clone(),
                admitted.geometry().clone(),
                None
            )
            .unwrap_err(),
        error
    );
}
