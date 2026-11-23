use super::*;
use worldrunner_render_plan::{TextureFilter, TextureSampler, TextureWrap};
use worldrunner_render_scene::simulation::UiTarget;
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
            .update(
                plan.clone(),
                screen(64, 4096),
                &textures,
                &images,
                |message| panic!("{message}"),
            )
            .unwrap();
        assert!(matches!(update, Some(RetainedUpdate::Ui(_))));
        assert_eq!(player.plan.as_ref(), Some(&plan));
        assert_eq!(player.updates, revision + 1);
        assert!(
            player
                .update(
                    plan.clone(),
                    screen(64, 4096),
                    &textures,
                    &images,
                    |message| { panic!("{message}") }
                )
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

fn screen(side: u32, max_texture: u32) -> Screen {
    Screen {
        size: [side, side],
        max_texture,
    }
}

/// The updates of `plan` three frames in a row on a square screen of 64 pixels whose device
/// takes textures up to `max_texture` pixels a side.
fn update_three_times(
    plan: &UiPresentation,
    max_texture: u32,
) -> (PlayerUi, Vec<String>, Vec<RetainedUpdate>) {
    let mut player = PlayerUi::default();
    let (textures, images) = (SpriteTextures::default(), PlayerTextures::default());
    let (mut lines, mut updates) = (Vec::new(), Vec::new());
    for _ in 0..3 {
        let viewport = screen(64, max_texture);
        let mut plan = plan.clone();
        player.prepare(&mut plan, viewport, &textures, &images, |line| {
            lines.push(line);
        });
        player.offer_targets(&mut plan, Vec::new(), |line| lines.push(line));
        let update = player.update(plan, viewport, &textures, &images, |line| lines.push(line));
        updates.extend(update.unwrap());
    }
    (player, lines, updates)
}

fn material_draw(material: UiMaterial, rect: [f32; 4]) -> UiDraw {
    let [x, y, width, height] = rect;
    UiDraw {
        material: Some(material),
        texture: Some(UiTexture::Image(1 << 40)),
        corners: [
            [x, y, 0.],
            [x + width, y, 0.],
            [x + width, y + height, 0.],
            [x, y + height, 0.],
        ],
        ..draw(7, 2, [0, 0, 64, 64])
    }
}

#[test]
fn a_draw_with_a_game_material_becomes_one_overlay_quad_and_the_work_that_draws_it() {
    let material = game_material();
    let plan = UiPresentation {
        draws: vec![
            draw(-3, 1, [0, 0, 64, 64]),
            material_draw(material, [8., 8., 16., 16.]),
            material_draw(material, [30., 8., 16., 16.]),
        ],
        shaders: BTreeMap::from([(material, "Game/Silhouette".to_owned())]),
        ..Default::default()
    };
    let (player, lines, updates) = update_three_times(&plan, 4096);
    assert!(lines.is_empty(), "{lines:?}");
    let [RetainedUpdate::Ui(overlay)] = updates.as_slice() else {
        panic!("one overlay for three identical frames");
    };
    assert_eq!(overlay.quad_count(), 2, "the default draw and one run");
    assert!(overlay.shows_layers());
    assert!(player.frame().is_some(), "the run is drawn every frame");
    assert_eq!(
        player.game_material_draws(),
        &BTreeMap::from([("Game/Silhouette".to_owned(), 2)])
    );
}

/// Three runs of 40 pixels square, apart by default draws, over a screen of 64 pixels square.
fn three_runs() -> UiPresentation {
    let material = game_material();
    UiPresentation {
        draws: vec![
            material_draw(material, [0., 0., 40., 40.]),
            draw(-3, 1, [0, 0, 64, 64]),
            material_draw(material, [0., 0., 40., 40.]),
            draw(-3, 1, [0, 0, 64, 64]),
            material_draw(material, [0., 0., 40., 40.]),
        ],
        shaders: BTreeMap::from([(material, "Game/Silhouette".to_owned())]),
        ..Default::default()
    }
}

#[test]
fn runs_that_do_not_fit_one_screen_of_atlas_are_all_drawn_by_a_larger_atlas() {
    let (player, lines, updates) = update_three_times(&three_runs(), 4096);
    assert!(lines.is_empty(), "{lines:?}");
    let [RetainedUpdate::Ui(overlay)] = updates.as_slice() else {
        panic!("one overlay");
    };
    assert_eq!(overlay.quad_count(), 5, "two default draws and three runs");
    assert!(player.frame().is_some());
}

#[test]
fn a_run_that_does_not_fit_the_atlas_at_the_device_limit_is_left_out_and_named_once() {
    let (player, lines, updates) = update_three_times(&three_runs(), 64);
    assert_eq!(
        lines,
        [
            "UI material 4294967299 (shader Game/Silhouette) is left out: its run of 40x40 pixels does not fit the layer atlas, which grows to at most 64 pixels a side"
        ],
        "one line for one material across three frames"
    );
    let [RetainedUpdate::Ui(overlay)] = updates.as_slice() else {
        panic!("one overlay");
    };
    assert_eq!(
        overlay.quad_count(),
        3,
        "two default draws and the one run that fits"
    );
    assert!(player.frame().is_some());
}

#[test]
fn a_default_draw_is_unchanged() {
    let plan = UiPresentation {
        draws: vec![draw(-3, 1, [0, 0, 64, 64]), draw(7, 2, [4, 4, 8, 8])],
        ..Default::default()
    };
    let (player, lines, _) = update_three_times(&plan, 4096);
    assert_eq!(player.plan.as_ref(), Some(&plan));
    assert!(player.frame().is_none());
    assert!(lines.is_empty());
    assert!(player.game_material_draws().is_empty());
}

#[test]
fn a_render_texture_draw_with_a_game_material_is_left_out_before_its_offer() {
    let material = game_material();
    let target = RenderTextureId::from_bits((1 << 32) | 4);
    let shaped = UiDraw {
        material: Some(material),
        texture: Some(UiTexture::Target(target)),
        ..draw(7, 2, [0, 0, 64, 64])
    };
    let mut plan = UiPresentation {
        draws: vec![draw(-3, 1, [0, 0, 64, 64]), shaped],
        shaders: BTreeMap::from([(material, "Game/Silhouette".to_owned())]),
        ..Default::default()
    };
    let sampler = TextureSampler {
        filter: TextureFilter::Point,
        wrap_u: TextureWrap::Clamp,
        wrap_v: TextureWrap::Clamp,
    };
    let offers = vec![(
        target,
        UiOffer::Written(UiTarget::new(target, None, sampler).unwrap()),
    )];
    let mut lines = Vec::new();
    let shown = PlayerUi::default().offer_targets(&mut plan, offers, |line| lines.push(line));
    assert_eq!(plan.draws.len(), 1);
    assert!(shown.is_empty(), "a left-out draw binds no texture");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("Game/Silhouette") && lines[0].contains("is left out"));
}
