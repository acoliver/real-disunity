use super::*;
use worldrunner_render_plan::*;
fn material(atlas: SceneImpostor) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(atlas)
}
#[test]
fn selectable_atlas_policies_preserve_default_bytes_and_validate_cutoffs() {
    let atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    let stock = MaterialUniform::from_material(&material(atlas)).to_bytes();
    let neutral = atlas
        .with_channels(SceneImpostorChannels::Material)
        .with_sampling(SceneAtlasSampling::MipZero)
        .with_capture_center([0; 3]);
    assert_eq!(
        stock,
        MaterialUniform::from_material(&material(neutral)).to_bytes()
    );
    assert_eq!(&stock[56..64], &[0; 8]);
    assert_eq!(&stock[308..320], &[0; 12]);
    assert_eq!(&stock[328..336], &[0; 8]);
    assert!(atlas.with_pass_cutoffs(1_000_001, 0).is_err());
    assert!(atlas.with_pass_cutoffs(0, u32::MAX).is_err());
    let bytes = MaterialUniform::from_material(&material(
        atlas.with_pass_cutoffs(970_000, 960_000).unwrap(),
    ))
    .to_bytes();
    assert_eq!(u32::from_le_bytes(bytes[56..60].try_into().unwrap()), 1);
    for (offset, expected) in [(328, 0.97_f32), (332, 0.96)] {
        assert!(
            (f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) - expected).abs()
                < 1e-7
        );
    }
}
#[test]
fn terminal_and_vertex_fold_encode_only_their_policy_lanes() {
    let atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    let fade = SceneFadeBand::new(2000, 4000).unwrap();
    assert!(!fade.terminal());
    assert!(SceneFadeBand::new(2, 2).is_err());
    let a = MaterialUniform::from_material(&material(atlas.with_fade(fade))).to_bytes();
    let b = MaterialUniform::from_material(&material(atlas.with_fade(fade.with_terminal_fade())))
        .to_bytes();
    assert_eq!(&a[..284], &b[..284]);
    assert_eq!(&a[288..], &b[288..]);
    assert_eq!(&b[284..288], &(-4.0_f32).to_le_bytes());
    let contact = SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 1)
        .unwrap()
        .with_normal_fold(1000, 0)
        .unwrap();
    let base = material(atlas).with_terrain_contact(contact);
    let default = MaterialUniform::from_material(&base).to_bytes();
    let explicit = MaterialUniform::from_material(
        &base.with_terrain_contact(contact.with_fold_sampling(SceneFoldSampling::InstanceOrigin)),
    )
    .to_bytes();
    assert_eq!(default, explicit);
    let vertex = MaterialUniform::from_material(
        &base.with_terrain_contact(contact.with_fold_sampling(SceneFoldSampling::Vertex)),
    )
    .to_bytes();
    assert_eq!(&default[..60], &vertex[..60]);
    assert_eq!(&default[64..], &vertex[64..]);
    assert_eq!(&vertex[60..64], &1_u32.to_le_bytes());
}

#[test]
fn minimum_arc_uses_reserved_lane_and_straight_default_bytes_are_unchanged() {
    let band = SceneWindBand::new(1_000_000, 0, 0).unwrap();
    let foliage = SceneFoliageWind::new(
        SceneDirection::new([0, 0, 1_000_000]).unwrap(),
        [1000; 2],
        1_000_000,
        [band; 2],
        SceneWindPhase::WorldPosition,
    )
    .unwrap();
    let wind = SceneWind::new(SceneDirection::new([1_000_000, 0, 0]).unwrap(), 0, 1000, 0).unwrap();
    let atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    let encode = |foliage| {
        MaterialUniform::from_material(
            &material(atlas)
                .with_features(SceneMaterialFeatures {
                    tint: SceneTint::WHITE,
                    ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
                    wind: Some(wind.with_foliage(foliage)),
                    layer: None,
                    translucency: None,
                })
                .unwrap(),
        )
        .to_bytes()
    };
    let stock = encode(foliage);
    assert_eq!(stock, encode(foliage.with_arc(SceneWindArc::StraightLimit)));
    assert_eq!(&stock[236..240], &[0; 4]);
    let minimum = encode(foliage.with_arc(SceneWindArc::MinimumHalfDegree));
    assert_eq!(&stock[..236], &minimum[..236]);
    assert_eq!(&stock[240..], &minimum[240..]);
    assert_eq!(&minimum[236..240], &1.0_f32.to_le_bytes());
}
