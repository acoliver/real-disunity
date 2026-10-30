use worldrunner_render_plan::{
    SceneAlphaMode, SceneMaterial, SceneMaterialParts, TextureAnisotropy, TextureHandle,
};

#[test]
fn anisotropy_accepts_only_supported_powers_of_two() {
    for level in [1, 2, 4, 8, 16] {
        assert_eq!(TextureAnisotropy::new(level).unwrap().level(), level);
    }
    for level in [0, 3, 6, 12, 17, u16::MAX] {
        assert!(TextureAnisotropy::new(level).is_err());
    }
}

#[test]
fn material_defaults_to_one_and_owns_anisotropy_independently_of_textures() {
    let default = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    assert_eq!(default.anisotropy().level(), 1);
    let authored = default.with_anisotropy(TextureAnisotropy::new(16).unwrap());
    assert_ne!(default, authored);
    assert_eq!(default.albedo(), authored.albedo());
    assert_eq!(authored.anisotropy().level(), 16);
}
