use super::*;
use crate::offscreen::ScreenImage;
use worldrunner_render_plan::ui::UiDraw;

fn draw(order: i32, hierarchy: usize, clip_rect: [u32; 4]) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        corner_colors: None,
        triangle_uv: None,
        corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        texture: None,
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

fn screen_image(frame: u64) -> ScreenImage {
    ScreenImage {
        width: 2,
        height: 2,
        rgba: vec![255; 16],
        frame,
    }
}

/// The key the overlay paints a draw by: lowest first, and in plan order among equal keys.
fn paint_key(draw: &UiDraw) -> (i32, i32, usize) {
    (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order)
}

#[test]
fn the_games_frame_is_one_draw_over_the_whole_viewport_ahead_of_the_games_ui() {
    let mut player = PlayerUi::default();
    let mut textures = SpriteTextures::default();
    let mut images = PlayerTextures::default();
    images.set_screen_image(&screen_image(0), &mut textures);
    let game = draw(i32::MIN, 0, [0, 0, 64, 48]);
    let plan = UiPresentation {
        draws: vec![game.clone()],
        ..Default::default()
    };

    let update = player
        .update(plan, [64, 48], &textures, &images, |message| {
            panic!("{message}")
        })
        .unwrap();

    assert!(matches!(update, Some(RetainedUpdate::Ui(_))));
    let held = &player.plan.as_ref().unwrap().draws;
    assert_eq!(held.len(), 2);
    assert_eq!(
        held[0],
        UiDraw {
            kind: UiDrawKind::Graphic,
            depth_test: false,
            corner_colors: None,
            triangle_uv: None,
            corners: [[0., 0., 0.], [64., 0., 0.], [64., 48., 0.], [0., 48., 0.]],
            uv_rect: [0., 0., 1., 1.],
            color: [1.; 4],
            texture: Some(SCREEN_IMAGE),
            sorting_layer: i32::MIN,
            sorting_order: i32::MIN,
            hierarchy_order: 0,
            clip_rect: [0, 0, 64, 48],
        }
    );
    assert_eq!(held[1], game);
    assert!(
        paint_key(&held[0]) <= paint_key(&held[1]),
        "the frame paints no later than the game's lowest draw, and first among equals"
    );
}

#[test]
fn without_an_image_of_the_screen_the_plan_is_the_games_alone() {
    let mut player = PlayerUi::default();
    let plan = UiPresentation {
        draws: vec![draw(3, 1, [0, 0, 64, 64])],
        ..Default::default()
    };
    player
        .update(
            plan.clone(),
            [64, 64],
            &SpriteTextures::default(),
            &PlayerTextures::default(),
            |message| panic!("{message}"),
        )
        .unwrap();
    assert_eq!(player.plan.as_ref(), Some(&plan));
}

#[test]
fn a_newer_image_or_a_new_viewport_replaces_the_ui_and_nothing_else_does() {
    let mut player = PlayerUi::default();
    let mut textures = SpriteTextures::default();
    let mut images = PlayerTextures::default();
    images.set_screen_image(&screen_image(0), &mut textures);
    let plan = UiPresentation::default();
    let mut changes = |viewport: [u32; 2], images: &PlayerTextures, textures: &SpriteTextures| {
        player
            .update(plan.clone(), viewport, textures, images, |message| {
                panic!("{message}")
            })
            .unwrap()
            .is_some()
    };

    assert!(changes([64, 48], &images, &textures), "the first frame");
    assert!(!changes([64, 48], &images, &textures), "the same image");
    images.set_screen_image(&screen_image(1), &mut textures);
    assert!(changes([64, 48], &images, &textures), "a newer image");
    assert!(changes([80, 48], &images, &textures), "a new window size");
    assert!(!changes([80, 48], &images, &textures));
}
