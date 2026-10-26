use worldrunner_render_plan::*;

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(0)),
        roughness_metallic: Some(TextureHandle::from_raw(0)),
        ambient_occlusion: Some(TextureHandle::from_raw(0)),
        self_illumination: Some(TextureHandle::from_raw(0)),
        roughness_micro: 750_000,
        metalness_micro: 250_000,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
}

#[test]
fn material_descriptors_preserve_every_base_field() {
    let base = material();
    let features = SceneMaterialFeatures {
        tint: SceneTint::new([250_000, 500_000, 1_000_000]).unwrap(),
        ambient_occlusion: SceneAmbientOcclusion::new(250_000, true).unwrap(),
        wind: None,
        layer: Some(
            SceneLayerBlend::new(MaterialHandle::from_raw(1), 750_000, SceneLayerMode::Height)
                .unwrap(),
        ),
    };
    let extended = base.with_features(features).unwrap();
    assert_eq!(extended.features(), Some(&features));
    assert_eq!(extended.albedo(), base.albedo());
    assert_eq!(extended.normal(), base.normal());
    assert_eq!(extended.roughness_metallic(), base.roughness_metallic());
    assert_eq!(extended.ambient_occlusion(), base.ambient_occlusion());
    assert_eq!(extended.self_illumination(), base.self_illumination());
    assert_eq!(extended.roughness_micro(), base.roughness_micro());
    assert_eq!(extended.metalness_micro(), base.metalness_micro());
    assert_eq!(extended.alpha_mode(), base.alpha_mode());
    assert_eq!(extended.double_sided(), base.double_sided());
    assert_eq!(extended, extended);
}

#[test]
fn descriptor_boundaries_are_closed() {
    assert!(SceneTint::new([0, 1_000_000, 500_000]).is_ok());
    assert!(SceneTint::new([2_550_000, SceneTint::MAX_CHANNEL_MICRO, 0]).is_ok());
    assert!(SceneTint::new([SceneTint::MAX_CHANNEL_MICRO + 1, 0, 0]).is_err());
    assert!(SceneAmbientOcclusion::new(1_000_001, true).is_err());
    assert!(
        SceneLayerBlend::new(
            MaterialHandle::from_raw(0),
            1_000_001,
            SceneLayerMode::Linear
        )
        .is_err()
    );
    let direction = SceneDirection::new([0, 1_000_000, 0]).unwrap();
    assert!(SceneWind::new(direction, 100, 0, 0).is_err());
    let wind = SceneWind::new(direction, 100, 2_000, 250_000).unwrap();
    assert_eq!(wind.direction(), direction);
    assert_eq!(wind.amplitude_millimeters(), 100);
    assert_eq!(wind.period_milliseconds(), 2_000);
    assert_eq!(wind.phase_micro(), 250_000);
    assert!(SceneCard::new(BillboardPolicy::YawLocked, [0, 1000], 100).is_err());
    assert!(SceneCard::new(BillboardPolicy::Spherical, [1000, 2000], 1_000_001).is_err());
}

#[test]
fn material_layer_uses_existing_material_identity() {
    let features = SceneMaterialFeatures {
        tint: SceneTint::WHITE,
        ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
        wind: None,
        layer: Some(
            SceneLayerBlend::new(MaterialHandle::from_raw(1), 500_000, SceneLayerMode::Linear)
                .unwrap(),
        ),
    };
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into_boxed_slice(),
    };
    let extended = material().with_features(features).unwrap();
    assert!(SceneResources::new(vec![texture.clone()], vec![extended]).is_err());
    assert!(SceneResources::new(vec![texture], vec![extended, material()]).is_ok());
}

#[test]
fn instance_tint_and_card_round_trip_without_basis_changes() {
    let transform = SceneTransform {
        position_millimeters: [-1000, 2000, 3000],
        rotation: SceneRotation::IDENTITY,
        scale_millionths: 2_000_000,
    };
    let tint = SceneTint::new([0, 500_000, 1_000_000]).unwrap();
    let object = SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_tint(tint);
    assert_eq!(object.tint(), tint);
    assert_eq!(object.transform(), &transform);
    let card = SceneCard::new(BillboardPolicy::YawLocked, [1000, 2000], 500_000).unwrap();
    let vertex = SceneVertex {
        tangent_micro: None,
        position_millimeters: [0; 3],
        normal_micro: [0, 0, 1_000_000],
        texcoord_micro: [0; 2],
    };
    let lod = SceneMeshLod::new(
        0,
        vec![vertex],
        vec![0, 0, 0],
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_card(card)
    .unwrap();
    assert_eq!(lod.card(), Some(&card));
    assert_eq!(lod.vertices(), &[vertex]);
    assert_eq!(lod.billboard(), Some(BillboardPolicy::YawLocked));
}

#[test]
fn water_body_validates_material_quantities_at_ingress() {
    let mut body = SceneWaterBody {
        material: crate::SceneWaterSettings::STOCK,
        surface: crate::SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: -500,
        color_micro: [0, 500_000, 1_000_000],
        absorption_micro: [0, 1_000_000, 10_000_000],
        roughness_micro: 500_000,
        east_min_millimeters: -1000,
        north_min_millimeters: 2000,
        east_extent_millimeters: 3000,
        north_extent_millimeters: 4000,
    };
    assert!(body.validate().is_ok());
    body.roughness_micro = 1_000_001;
    assert!(body.validate().is_err());
    body.roughness_micro = 0;
    body.absorption_micro[0] = 10_000_001;
    assert!(body.validate().is_err());
}

#[test]
fn typed_quantities_round_trip_without_losing_components() {
    let tint = SceneTint::new([17, 500_000, 999_999]).unwrap();
    assert_eq!(SceneTint::new(tint.channels()).unwrap(), tint);
    let ao = SceneAmbientOcclusion::new(123_456, true).unwrap();
    assert_eq!(
        SceneAmbientOcclusion::new(ao.factor_micro(), ao.vertex()).unwrap(),
        ao
    );
    let direction = SceneDirection::new([0, -1_000_000, 0]).unwrap();
    let wind = SceneWind::new(direction, 234, 3210, 432_100).unwrap();
    assert_eq!(
        SceneWind::new(
            wind.direction(),
            wind.amplitude_millimeters(),
            wind.period_milliseconds(),
            wind.phase_micro()
        )
        .unwrap(),
        wind
    );
    assert_eq!(wind.direction().components_micro(), [0, -1_000_000, 0]);
    let card = SceneCard::new(BillboardPolicy::Spherical, [123, 456], 765_432).unwrap();
    assert_eq!(
        SceneCard::new(
            card.policy(),
            card.size_millimeters(),
            card.coverage_micro()
        )
        .unwrap(),
        card
    );
    let layer =
        SceneLayerBlend::new(MaterialHandle::from_raw(9), 321_000, SceneLayerMode::Height).unwrap();
    assert_eq!(
        SceneLayerBlend::new(layer.material(), layer.weight_micro(), layer.mode()).unwrap(),
        layer
    );
}
