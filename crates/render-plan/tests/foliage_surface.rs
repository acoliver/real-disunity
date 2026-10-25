use worldrunner_render_plan::*;

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
}

#[test]
fn transmission_and_contact_inputs_validate_ranges_without_changing_stock_channels() {
    let base = material();
    assert_eq!(base.translucency_micro(), 0);
    assert_eq!(base.terrain_contact(), None);
    for value in [0, 1_000_000] {
        let transmitted = base.with_translucency(value).unwrap();
        assert_eq!(transmitted.translucency_micro(), value);
        assert_eq!(transmitted.albedo(), base.albedo());
        assert_eq!(transmitted.alpha_mode(), base.alpha_mode());
    }
    assert!(base.with_translucency(1_000_001).is_err());
    for (roughness, metalness, distance) in [(1_000_001, 0, 1), (0, 1_000_001, 1), (0, 0, 0)] {
        assert!(
            SceneTerrainContact::new(SceneTint::WHITE, roughness, metalness, distance).is_err()
        );
    }
    let contact = SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 1000).unwrap();
    let contacted = base.with_terrain_contact(contact);
    assert_eq!(contacted.terrain_contact(), Some(contact));
    assert_eq!(contacted.albedo(), base.albedo());
}
