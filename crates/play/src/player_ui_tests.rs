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
    ui.update(plan.clone(), [width, 64], textures, player, |_| {})
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

#[test]
fn unavailable_images_leave_other_ui_draws_in_the_frame() {
    let (mut plan, textures) = fixture();
    let mut solid = plan.draws[0].clone();
    solid.texture = None;
    plan.draws.push(solid);
    for identity in [7, FIRST_PLAYER_TEXTURE + 1, 7] {
        let mut missing = plan.draws[0].clone();
        missing.texture = Some(identity);
        plan.draws.push(missing);
    }
    let mut ui = PlayerUi::default();
    let player = PlayerTextures::default();
    let mut reports = vec![];
    for frame in 0..3 {
        let update = ui
            .update(plan.clone(), [64, 64], &textures, &player, |line| {
                reports.push(line);
            })
            .unwrap();
        assert_eq!(update.is_some(), frame == 0);
    }
    assert_eq!(
        reports,
        [
            "UI image 7 is left out: scene texture identity is absent from the sprite textures",
            "UI image 1099511627777 is left out: no texture was stored by the managed host",
        ]
    );
}

#[test]
fn a_later_available_image_replaces_the_filtered_frame() {
    let (plan, mut textures) = fixture();
    let texture = textures.textures.remove(&FIRST_PLAYER_TEXTURE).unwrap();
    let player = PlayerTextures::default();
    let mut ui = PlayerUi::default();
    assert!(changes(&mut ui, &plan, 64, &textures, &player));
    assert!(!changes(&mut ui, &plan, 64, &textures, &player));
    textures.textures.insert(FIRST_PLAYER_TEXTURE, texture);
    assert!(changes(&mut ui, &plan, 64, &textures, &player));
    assert!(!changes(&mut ui, &plan, 64, &textures, &player));
}

fn stored(generation: u64) -> worldrunner_disunity_runtime::PlayerTexture {
    worldrunner_disunity_runtime::PlayerTexture {
        format: 4,
        width: 1,
        height: 1,
        mips: 1,
        linear: true,
        generation,
        distance_spread: 0,
        bytes: vec![255; 4],
    }
}

#[test]
fn conversion_failure_removes_the_image_and_later_storage_restores_it() {
    let (plan, mut textures) = fixture();
    let id = FIRST_PLAYER_TEXTURE;
    let mut player = PlayerTextures::default();
    let mut ui = PlayerUi::default();
    player.convert(id, &stored(1), &mut textures);
    assert!(changes(&mut ui, &plan, 64, &textures, &player));
    let mut short = stored(2);
    short.bytes.clear();
    player.convert(id, &short, &mut textures);
    let mut reports = vec![];
    for frame in 0..3 {
        let update = ui
            .update(plan.clone(), [64, 64], &textures, &player, |line| {
                reports.push(line);
            })
            .unwrap();
        assert_eq!(update.is_some(), frame == 0);
    }
    assert_eq!(
        reports,
        [format!(
            "UI image {id} is left out: texture conversion failed: a 1x1 RGBA32 texture holds 0 bytes but its base level needs 4"
        )]
    );
    player.convert(id, &stored(3), &mut textures);
    assert!(!player.failures.contains_key(&id));
    assert!(changes(&mut ui, &plan, 64, &textures, &player));
    assert!(!changes(&mut ui, &plan, 64, &textures, &player));
}

#[test]
fn unavailable_text_coverage_is_still_an_error() {
    let (mut plan, textures) = fixture();
    plan.draws[0].kind = UiDrawKind::Text;
    let error = PlayerUi::default()
        .update(
            plan,
            [64, 64],
            &textures,
            &PlayerTextures::default(),
            |_| {},
        )
        .unwrap_err();
    assert!(error.to_string().contains("glyph coverage texture"));
}

#[test]
fn generic_overlay_still_rejects_unresolved_graphics() {
    let (plan, _) = fixture();
    let error =
        worldrunner_render_scene::simulation::UiOverlay::from_presentation(&plan, &BTreeMap::new())
            .unwrap_err();
    assert!(error.to_string().contains("unresolved UI texture identity"));
}
