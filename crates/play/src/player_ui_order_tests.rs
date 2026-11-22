use super::*;
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::presentation::{UiDraw, UiMaterial};

fn draw(order: i32, hierarchy: usize, clip_rect: [u32; 4]) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        corner_colors: None,
        distance_field: None,
        triangle_uv: None,
        corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        texture: None,
        material: None,
        sorting_layer: 2,
        sorting_order: order,
        hierarchy_order: hierarchy,
        clip_rect,
    }
}

#[test]
fn exported_ui_replaces_retained_contributions_when_only_order_or_clip_changes() {
    let mut player = PlayerUi::default();
    let textures = SpriteTextures::default();
    let images = PlayerTextures::default();
    let mut plan = UiPresentation {
        draws: vec![draw(-3, 2, [0, 0, 64, 64]), draw(7, 1, [12, 12, 8, 8])],
        ..Default::default()
    };
    for revision in 0..3 {
        let update = player
            .update(plan.clone(), [64, 64], &textures, &images, |message| {
                panic!("{message}")
            })
            .unwrap();
        assert!(matches!(update, Some(RetainedUpdate::Ui(_))));
        assert_eq!(player.plan.as_ref(), Some(&plan));
        assert_eq!(player.updates, revision + 1);
        assert!(
            player
                .update(plan.clone(), [64, 64], &textures, &images, |message| {
                    panic!("{message}")
                })
                .unwrap()
                .is_none()
        );
        if revision == 0 {
            plan.draws[1].sorting_order = 9;
            plan.draws[1].hierarchy_order = 3;
        } else {
            plan.draws[1].clip_rect = [14, 14, 4, 4];
        }
    }
}

fn game_material() -> UiMaterial {
    UiMaterial {
        id: MaterialId::from_bits((1 << 32) | 3),
        generation: 2,
    }
}

fn update_three_times(plan: &UiPresentation) -> (PlayerUi, Vec<String>) {
    let mut player = PlayerUi::default();
    let (textures, images) = (SpriteTextures::default(), PlayerTextures::default());
    let mut lines = Vec::new();
    for _ in 0..3 {
        let viewport = [64, 64];
        let update = player.update(plan.clone(), viewport, &textures, &images, |line| {
            lines.push(line);
        });
        update.unwrap();
    }
    (player, lines)
}

#[test]
fn a_draw_with_a_game_material_is_left_out_and_named_once() {
    let material = game_material();
    let shaped = UiDraw {
        material: Some(material),
        texture: Some(1 << 40),
        ..draw(7, 2, [0, 0, 64, 64])
    };
    let plan = UiPresentation {
        draws: vec![draw(-3, 1, [0, 0, 64, 64]), shaped.clone(), shaped],
        shaders: BTreeMap::from([(material, "Game/Silhouette".to_owned())]),
        ..Default::default()
    };
    let (player, lines) = update_three_times(&plan);
    assert_eq!(
        player.plan.as_ref().unwrap().draws.as_slice(),
        &plan.draws[..1]
    );
    assert_eq!(
        lines,
        [
            "UI material 4294967299 (shader Game/Silhouette) is left out: UI draws with a game shader are not drawn yet"
        ],
        "the material is named once and its unavailable texture is not named"
    );
    assert_eq!(
        player.game_material_draws(),
        &BTreeMap::from([("Game/Silhouette".to_owned(), 2)])
    );
}

#[test]
fn a_default_draw_is_unchanged() {
    let plan = UiPresentation {
        draws: vec![draw(-3, 1, [0, 0, 64, 64]), draw(7, 2, [4, 4, 8, 8])],
        ..Default::default()
    };
    let (player, lines) = update_three_times(&plan);
    assert_eq!(player.plan.as_ref(), Some(&plan));
    assert!(lines.is_empty());
    assert!(player.game_material_draws().is_empty());
}
