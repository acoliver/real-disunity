use super::*;
use crate::player_ui::{PlayerUi, Screen};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_scene_viewer::RetainedUpdate;
use worldrunner_world_simulation::ui::UiTexture;
use worldrunner_world_simulation::ui::presentation::{UiDraw, UiDrawKind, UiPresentation};

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
            corner_colors: None,
            distance_field: None,
            triangle_uv: None,
            corners: [[0., 0., 0.], [8., 0., 0.], [8., 8., 0.], [0., 8., 0.]],
            uv_rect: [0., 0., 1., 1.],
            color: [1.; 4],
            texture: Some(UiTexture::Image(identity)),
            material: None,
            sorting_layer: 0,
            sorting_order: 0,
            hierarchy_order: 1,
            clip_rect: [0, 0, 8, 8],
        }],
        ..Default::default()
    };
    (plan, textures)
}

/// A square screen of 64 pixels on a device that takes large textures.
fn square() -> Screen {
    Screen {
        size: [64, 64],
        max_texture: 4096,
    }
}

fn changes(
    ui: &mut PlayerUi,
    plan: &UiPresentation,
    width: u32,
    textures: &SpriteTextures,
    player: &PlayerTextures,
) -> bool {
    ui.update(
        plan.clone(),
        Screen {
            size: [width, 64],
            max_texture: 4096,
        },
        textures,
        player,
        |_| {},
    )
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
        missing.texture = Some(UiTexture::Image(identity));
        plan.draws.push(missing);
    }
    let mut ui = PlayerUi::default();
    let player = PlayerTextures::default();
    let mut reports = vec![];
    for frame in 0..3 {
        let update = ui
            .update(plan.clone(), square(), &textures, &player, |line| {
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
        depth: 0,
        dimension: worldrunner_disunity_runtime::TextureDimension::D2,
        graphics_format: 0,
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
            .update(plan.clone(), square(), &textures, &player, |line| {
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
    plan.draws[0].texture = Some(UiTexture::Coverage(FIRST_PLAYER_TEXTURE));
    let error = PlayerUi::default()
        .update(
            plan,
            square(),
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
    let error = worldrunner_render_scene::simulation::UiOverlay::from_presentation(
        &plan,
        &BTreeMap::new(),
        &worldrunner_world_simulation::ui::presentation::UiLayers::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unresolved UI texture identity"));
}

mod targets {
    use super::*;
    use crate::gpu_player::UiOffer;
    use worldrunner_render_plan::{TextureFilter, TextureSampler, TextureWrap};
    use worldrunner_render_scene::simulation::UiTarget;
    use worldrunner_world_simulation::graphics::RenderTextureId;

    fn id(bits: u64) -> RenderTextureId {
        RenderTextureId::from_bits(bits)
    }

    fn target(bits: u64) -> UiTarget {
        let sampler = TextureSampler {
            filter: TextureFilter::Point,
            wrap_u: TextureWrap::Clamp,
            wrap_v: TextureWrap::Clamp,
        };
        UiTarget::new(id(bits), None, sampler).unwrap()
    }

    fn over(bits: u64) -> UiDraw {
        let (plan, _) = fixture();
        UiDraw {
            texture: Some(UiTexture::Target(id(bits))),
            ..plan.draws[0].clone()
        }
    }

    fn plan_of(bits: &[u64]) -> UiPresentation {
        UiPresentation {
            draws: bits.iter().map(|bits| over(*bits)).collect(),
            ..UiPresentation::default()
        }
    }

    #[test]
    fn a_draw_whose_texture_the_host_answers_for_stays_and_its_texture_is_offered() {
        let mut ui = PlayerUi::default();
        let mut plan = plan_of(&[1, 2]);
        let mut reports = Vec::new();
        let offers = vec![
            (id(1), UiOffer::Written(target(1))),
            (id(2), UiOffer::Unwritten(target(2))),
        ];
        let offered = ui.offer_targets(&mut plan, offers, |line| reports.push(line));
        assert_eq!(
            offered.iter().map(UiTarget::id).collect::<Vec<_>>(),
            [id(1), id(2)]
        );
        assert_eq!(plan.draws.len(), 2);
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert!(reports[0].contains("shows nothing until a frame draws it"));
    }

    #[test]
    fn a_draw_over_an_unavailable_texture_is_left_out_and_named_once() {
        let mut ui = PlayerUi::default();
        let mut reports = Vec::new();
        for _ in 0..3 {
            let mut plan = plan_of(&[1, 2]);
            let offers = vec![
                (id(1), UiOffer::Unavailable("destroyed")),
                (id(2), UiOffer::Unavailable("multisampled")),
            ];
            let offered = ui.offer_targets(&mut plan, offers, |line| reports.push(line));
            assert!(offered.is_empty());
            assert!(plan.draws.is_empty());
        }
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert!(reports[0].contains("left out: destroyed"));
        assert!(reports[1].contains("left out: multisampled"));
    }

    #[test]
    fn an_image_draw_is_not_touched_by_the_offer() {
        let (plan, _) = fixture();
        let mut kept = plan.clone();
        let mut ui = PlayerUi::default();
        let offered = ui.offer_targets(&mut kept, Vec::new(), |_| {});
        assert!(offered.is_empty());
        assert_eq!(kept, plan);
    }

    #[test]
    fn the_overlay_the_player_builds_names_the_targets_the_offer_answers() {
        let mut ui = PlayerUi::default();
        let mut plan = plan_of(&[1]);
        let offers = vec![(id(1), UiOffer::Written(target(1)))];
        ui.offer_targets(&mut plan, offers, |_| {});
        let textures = SpriteTextures::default();
        let update = ui
            .update(
                plan,
                square(),
                &textures,
                &PlayerTextures::default(),
                |_| {},
            )
            .unwrap();
        let Some(RetainedUpdate::Ui(overlay)) = update else {
            panic!("a draw over an offered target changes the overlay");
        };
        assert_eq!(overlay.targets(), [id(1)]);
    }
}
