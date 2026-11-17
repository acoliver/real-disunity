//! Texture tiers on a prepared scene: only eligible textures lose levels, all by the same count.
use super::*;
use worldrunner_render_plan::TexelFormat;

/// An RGBA8 square of `size` with its full chain down to 1x1.
fn chain(size: u32) -> SceneTexture {
    let levels = size.ilog2() + 1;
    let bytes: usize = (0..levels)
        .map(|level| 4 * usize::try_from(size >> level).expect("small side").pow(2))
        .sum();
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: size,
        height: size,
        mips: levels,
        bytes: vec![128; bytes].into(),
    }
}

/// One quad whose material samples texture 0, and a second texture the scene also holds.
fn prepared() -> super::super::PreparedScene {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![chain(64), chain(64)],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad()],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap();
    super::super::PreparedScene::new(&scene).unwrap()
}

fn widths(prepared: &super::super::PreparedScene) -> Vec<u32> {
    prepared
        .textures
        .iter()
        .map(|texture| texture.as_prepared().width())
        .collect()
}

const POLICY: crate::texture::TextureTierPolicy = crate::texture::TextureTierPolicy {
    budget_fraction_micro: 1_000_000,
    max_dropped_levels: 2,
};

#[test]
fn only_eligible_textures_lose_the_chosen_levels() {
    let mut scene = prepared();
    let authored = scene.textures[0].as_prepared().tier_bytes(0);
    // The budget holds one 32x32 chain beside 1 KiB of other resident memory.
    let budget = scene.textures[0].as_prepared().tier_bytes(1) + 1024;
    let first = |handle: TextureHandle| handle.raw() == 0;
    let choice = scene.apply_texture_tier(Some(POLICY), Some(budget), 1024, first);
    assert_eq!(choice.dropped_levels, 1);
    assert_eq!(choice.authored_bytes, authored);
    assert_eq!(choice.saved_bytes(), authored - choice.kept_bytes);
    assert_eq!(widths(&scene), [32, 64]);
    assert_eq!(scene.textures[0].as_prepared().mips().len(), 6);
}

#[test]
fn without_a_policy_the_prepared_scene_is_unchanged() {
    let mut scene = prepared();
    let choice = scene.apply_texture_tier(None, Some(1), 0, |_| true);
    assert_eq!(choice.dropped_levels, 0);
    assert_eq!(choice.kept_bytes, choice.authored_bytes);
    assert_eq!(widths(&scene), [64, 64]);
}
