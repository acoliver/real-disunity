use super::*;
use crate::player_ui::PlayerUi;
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace,
    ui::{UiDraw, UiDrawKind, UiPresentation},
};

fn fixture() -> (UiPresentation, SpriteTextures) {
    let identity = FIRST_PLAYER_TEXTURE;
    let mut textures = SpriteTextures::default();
    textures.textures.insert(
        identity,
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![255; 4].into(),
        },
    );
    let plan = UiPresentation {
        draws: vec![UiDraw {
            kind: UiDrawKind::Graphic,
            depth_test: false,
            triangle_uv: None,
            corners: [[0., 0., 0.], [8., 0., 0.], [8., 8., 0.], [0., 8., 0.]],
            uv_rect: [0., 0., 1., 1.],
            color: [1.; 4],
            texture: Some(identity),
            sorting_layer: 0,
            sorting_order: 0,
            hierarchy_order: 1,
            clip_rect: [0, 0, 8, 8],
        }],
        ..Default::default()
    };
    (plan, textures)
}

fn changes(
    ui: &mut PlayerUi,
    plan: &UiPresentation,
    width: u32,
    textures: &SpriteTextures,
    player: &PlayerTextures,
) -> bool {
    ui.update(plan.clone(), [width, 64], textures, player)
        .unwrap()
        .is_some()
}

#[test]
fn ui_cache_tracks_texture_generations_viewport_draws_and_clear() {
    let mut player = PlayerTextures::default();
    player.generations.insert(FIRST_PLAYER_TEXTURE, 1);
    let (mut plan, textures) = fixture();
    let mut ui = PlayerUi::default();
    assert!(changes(&mut ui, &plan, 64, &textures, &player));
    assert!(!changes(&mut ui, &plan, 64, &textures, &player));
    player.generations.insert(FIRST_PLAYER_TEXTURE, 2);
    assert!(changes(&mut ui, &plan, 64, &textures, &player));
    assert!(!changes(&mut ui, &plan, 64, &textures, &player));
    assert!(changes(&mut ui, &plan, 128, &textures, &player));
    plan.draws[0].clip_rect[2] = 4;
    assert!(changes(&mut ui, &plan, 128, &textures, &player));
    plan.draws.clear();
    assert!(changes(&mut ui, &plan, 128, &textures, &player));
    assert!(!changes(&mut ui, &plan, 128, &textures, &player));
    assert_eq!(ui.updates, 5);
}
