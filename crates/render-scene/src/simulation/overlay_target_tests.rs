use super::*;
use worldrunner_world_simulation::graphics::RenderTextureId;
use worldrunner_world_simulation::ui::UiTexture;

fn target(bits: u64) -> RenderTextureId {
    RenderTextureId::from_bits(bits)
}

fn draw(texture: UiTexture) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [[0., 0., 0.], [8., 0., 0.], [8., 8., 0.], [0., 8., 0.]],
        uv_rect: [0.1, 0.2, 0.6, 0.9],
        color: [1.; 4],
        corner_colors: None,
        distance_field: None,
        texture: Some(texture),
        material: None,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, 8, 8],
    }
}

fn overlay(draws: Vec<UiDraw>, textures: &BTreeMap<u64, SceneTexture>) -> UiOverlay {
    let presentation = UiPresentation {
        draws,
        ..UiPresentation::default()
    };
    let layers = worldrunner_world_simulation::ui::presentation::UiLayers::default();
    UiOverlay::from_presentation(&presentation, textures, &layers).unwrap()
}

fn near(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-6, "{actual:?} is not {expected:?}");
    }
}

#[test]
fn a_target_draw_takes_a_slot_and_flips_v() {
    let shown = overlay(vec![draw(UiTexture::Target(target(5)))], &BTreeMap::new());
    assert_eq!(shown.targets(), [target(5)]);
    near(&shown.quads[0].uv_rect, &[0.1, 0.8, 0.6, 0.1]);
    let mut triangle = draw(UiTexture::Target(target(5)));
    triangle.triangle_uv = Some([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]);
    let shown = overlay(vec![triangle], &BTreeMap::new());
    let flipped = shown.quads[0].triangle_uv.unwrap();
    near(&flipped.concat(), &[0.1, 0.8, 0.3, 0.6, 0.5, 0.4]);
}

#[test]
fn two_draws_of_one_target_share_a_slot() {
    let shown = overlay(
        vec![
            draw(UiTexture::Target(target(5))),
            draw(UiTexture::Target(target(7))),
            draw(UiTexture::Target(target(5))),
        ],
        &BTreeMap::new(),
    );
    assert_eq!(shown.targets(), [target(5), target(7)]);
    let slots: Vec<_> = shown.quads.iter().map(|q| q.texture).collect();
    assert_eq!(slots, [Some(0), Some(1), Some(0)]);
}

#[test]
fn a_target_draw_needs_no_texture_in_the_overlay_table() {
    let shown = overlay(vec![draw(UiTexture::Target(target(5)))], &BTreeMap::new());
    assert!(shown.textures.is_empty());
}

#[test]
fn target_slots_follow_the_overlays_own_textures() {
    let image = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into(),
    };
    let textures = BTreeMap::from([(1 << 40, image)]);
    let shown = overlay(
        vec![
            draw(UiTexture::Target(target(5))),
            draw(UiTexture::Image(1 << 40)),
        ],
        &textures,
    );
    assert_eq!(shown.textures.len(), 1);
    let slots: Vec<_> = shown.quads.iter().map(|q| q.texture).collect();
    assert_eq!(
        slots,
        [Some(1), Some(0)],
        "the target's slot is counted after the image table, though it was drawn first"
    );
    near(&shown.quads[1].uv_rect, &[0.1, 0.2, 0.6, 0.9]);
}
