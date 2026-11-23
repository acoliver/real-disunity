use super::*;
use worldrunner_world_simulation::{graphics::RenderTextureId, ui::UiTexture};

#[test]
fn triangle_copy_out_keeps_uvs_topology_clip_and_draw_order() {
    for depth_test in [false, true] {
        triangle_copy_out(depth_test);
    }
}
fn triangle_copy_out(depth_test: bool) {
    use worldrunner_world_simulation::ui::presentation::UiPresentation;
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    let draw = UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test,
        corner_colors: None,
        distance_field: None,
        triangle_uv: Some([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]),
        corners: [
            [10., 10., 0.],
            [20., 10., 0.],
            [20., 20., 0.],
            [20., 20., 0.],
        ],
        uv_rect: [0.; 4],
        color: [1.; 4],
        texture: None,
        material: None,
        sorting_layer: 2,
        sorting_order: 3,
        hierarchy_order: 4,
        clip_rect: [5, 6, 7, 8],
    };
    crate::lock_store()
        .session_mut(world)
        .unwrap()
        .ui_presentation = Some(UiPresentation {
        draws: vec![draw],
        ..Default::default()
    });
    let mut meta = DisunityUiDraw::default();
    let mut quad = DisunityQuad::default();
    assert_eq!(
        unsafe { disunity_presentation_ui_draw(world, 0, &mut meta, &mut quad) }.code,
        0
    );
    assert_eq!(meta.kind, 2 | (u32::from(depth_test) << 2));
    assert_eq!(
        [
            [meta.triangle_u0, meta.triangle_v0],
            [meta.triangle_u1, meta.triangle_v1],
            [meta.triangle_u2, meta.triangle_v2],
        ],
        [[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]
    );
    assert_eq!(
        (quad.sorting_layer, quad.sorting_order, meta.hierarchy_order),
        (2, 3, 4)
    );
    assert_eq!(
        [meta.clip_x, meta.clip_y, meta.clip_width, meta.clip_height],
        [5, 6, 7, 8]
    );
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}

#[test]
fn ui_copy_out_requires_a_prepared_live_frame_and_valid_outputs() {
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    let (mut draws, mut textures) = (99, 99);
    let mut header = crate::presentation::DisunityPresentation::default();
    let mut draw = DisunityUiDraw::default();
    let mut quad = DisunityQuad::default();
    let mut coverage = DisunityCoverage::default();
    unsafe {
        assert_ne!(
            disunity_presentation_ui_count(world, &mut draws, &mut textures).code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_count(world, std::ptr::null_mut(), &mut textures).code,
            0
        );
        assert_eq!(
            crate::presentation::disunity_presentation_prepare(world, 64, 64, 8, &mut header).code,
            0
        );
        assert_eq!(
            disunity_presentation_ui_count(world, &mut draws, &mut textures).code,
            0
        );
        assert_eq!((draws, textures), (0, 0));
        assert_ne!(
            disunity_presentation_ui_draw(world, 0, &mut draw, &mut quad).code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_draw(world, 0, std::ptr::null_mut(), &mut quad).code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_coverage(world, 0, &mut coverage, std::ptr::null_mut(), 0)
                .code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_coverage(world, 0, &mut coverage, std::ptr::null_mut(), 1)
                .code,
            0
        );
    }
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
    assert_ne!(
        unsafe { disunity_presentation_ui_count(world, &mut draws, &mut textures) }.code,
        0
    );
}

fn snapshot_draw(order: i32, hierarchy: usize) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        corner_colors: None,
        distance_field: None,
        triangle_uv: None,
        corners: [[0.; 3]; 4],
        uv_rect: [0.; 4],
        color: [1.; 4],
        texture: None,
        material: None,
        sorting_layer: 0,
        sorting_order: order,
        hierarchy_order: hierarchy,
        clip_rect: [0, 0, 64, 64],
    }
}

fn count_snapshot(draws: Vec<UiDraw>) -> (i32, u32, u32) {
    let last = u32::try_from(draws.len() - 1).unwrap();
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    crate::lock_store()
        .session_mut(world)
        .unwrap()
        .ui_presentation = Some(
        worldrunner_world_simulation::ui::presentation::UiPresentation {
            draws,
            ..Default::default()
        },
    );
    let (mut draws, mut textures) = (99, 99);
    let result = unsafe { disunity_presentation_ui_count(world, &mut draws, &mut textures) };
    if result.code != 0 {
        assert_draw_rejected(world, last);
    }
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
    (result.code, draws, textures)
}

fn assert_draw_rejected(world: u64, index: u32) {
    let mut meta = DisunityUiDraw {
        kind: 99,
        ..Default::default()
    };
    let mut quad = DisunityQuad {
        sorting_order: 99,
        ..Default::default()
    };
    assert_ne!(
        unsafe { disunity_presentation_ui_draw(world, index, &mut meta, &mut quad) }.code,
        0
    );
    assert_eq!(
        (meta.kind, quad.sorting_order),
        (99, 99),
        "failure changes no outputs"
    );
}

#[test]
fn ui_copy_out_rejects_descending_canvas_sibling_and_kind_order() {
    let mut text = snapshot_draw(0, 1);
    text.kind = UiDrawKind::Text;
    let mut higher_layer = snapshot_draw(0, 1);
    higher_layer.sorting_layer = 1;
    for draws in [
        vec![higher_layer, snapshot_draw(7, 0)],
        vec![snapshot_draw(7, 0), snapshot_draw(-3, 1)],
        vec![snapshot_draw(0, 2), snapshot_draw(0, 1)],
        vec![text, snapshot_draw(0, 1)],
    ] {
        let (code, draws, textures) = count_snapshot(draws);
        assert_ne!(
            code, 0,
            "malformed draw order must fail at the ABI boundary"
        );
        assert_eq!((draws, textures), (99, 99), "failure changes no outputs");
    }
}

#[test]
fn ui_copy_out_rejects_overflowing_clip_records() {
    for clip_rect in [[u32::MAX, 0, 1, 1], [0, u32::MAX, 1, 1]] {
        let mut draw = snapshot_draw(0, 0);
        draw.clip_rect = clip_rect;
        let (code, draws, textures) = count_snapshot(vec![draw]);
        assert_ne!(code, 0, "overflowing clip must fail at the ABI boundary");
        assert_eq!((draws, textures), (99, 99));
    }
}

#[test]
fn ui_copy_out_accepts_signed_sort_orders_and_empty_clips() {
    let mut empty = snapshot_draw(i32::MAX, 2);
    empty.clip_rect = [64, 64, 0, 0];
    assert_eq!(
        count_snapshot(vec![snapshot_draw(i32::MIN, 1), empty]),
        (0, 2, 0)
    );
}

#[test]
fn ui_copy_out_rejects_invalid_corner_channels() {
    for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        for corner in 0..4 {
            for channel in 0..4 {
                let mut draw = snapshot_draw(0, 0);
                let mut colors = [[1.; 4]; 4];
                colors[corner][channel] = value;
                draw.corner_colors = Some(colors);
                let (code, draws, textures) = count_snapshot(vec![draw]);
                assert_ne!(code, 0, "invalid corner channel must fail at copy-out");
                assert_eq!((draws, textures), (99, 99));
            }
        }
    }
}

#[test]
fn corner_metadata_packs_all_channels_without_folding_common_tint() {
    let colors = std::array::from_fn(|i| std::array::from_fn(|c| (i * 4 + c) as f32 / 16.));
    let mut draw = snapshot_draw(0, 0);
    draw.color = [0.25; 4];
    draw.corner_colors = Some(colors);
    let meta = encode_metadata(&draw);
    assert_eq!(meta.corner_colors_present, 1);
    assert_eq!(metadata_colors(meta), colors);
    assert_eq!(encode(&draw).r, 0.25);
    draw.corner_colors = None;
    let meta = encode_metadata(&draw);
    assert_eq!(meta.corner_colors_present, 0);
    assert_eq!(metadata_colors(meta), [[1.; 4]; 4]);
}

fn metadata_colors(meta: DisunityUiDraw) -> [[f32; 4]; 4] {
    [
        [
            meta.corner_0_r,
            meta.corner_0_g,
            meta.corner_0_b,
            meta.corner_0_a,
        ],
        [
            meta.corner_1_r,
            meta.corner_1_g,
            meta.corner_1_b,
            meta.corner_1_a,
        ],
        [
            meta.corner_2_r,
            meta.corner_2_g,
            meta.corner_2_b,
            meta.corner_2_a,
        ],
        [
            meta.corner_3_r,
            meta.corner_3_g,
            meta.corner_3_b,
            meta.corner_3_a,
        ],
    ]
}

#[test]
fn distance_metadata_keeps_its_layout_at_revision_12() {
    use std::mem::{align_of, offset_of, size_of};
    assert_eq!(crate::abi::DISUNITY_ABI_VERSION, 31);
    assert_eq!(size_of::<DisunityUiDraw>(), 176);
    assert_eq!(align_of::<DisunityUiDraw>(), 8);
    assert_eq!(offset_of!(DisunityUiDraw, distance_mode), 124);
    assert_eq!(offset_of!(DisunityUiDraw, distance_spread), 128);
    assert_eq!(offset_of!(DisunityUiDraw, distance_radius), 132);
    assert_eq!(offset_of!(DisunityUiDraw, effect_r), 136);
    assert_eq!(offset_of!(DisunityUiDraw, effect_a), 148);
    assert_eq!(offset_of!(DisunityUiDraw, texture_kind), 152);
    assert_eq!(offset_of!(DisunityUiDraw, material), 160);
    assert_eq!(offset_of!(DisunityUiDraw, material_generation), 168);
}

#[test]
fn distance_metadata_encodes_modes_and_rejects_invalid_styles() {
    use worldrunner_world_simulation::ui::presentation::{UiDistanceMode, UiDistanceStyle};
    let mut draw = snapshot_draw(0, 0);
    draw.kind = UiDrawKind::Text;
    draw.texture = Some(UiTexture::Coverage(7));
    draw.corners = [[0., 0., 0.], [16., 0., 0.], [16., 16., 0.], [0., 16., 0.]];
    draw.uv_rect = [0., 0., 1., 1.];
    for (mode, code) in [
        (UiDistanceMode::Fill, 1),
        (UiDistanceMode::Outline, 2),
        (UiDistanceMode::Glow, 3),
    ] {
        let style = UiDistanceStyle {
            spread_px: 8.,
            radius_px: 2.,
            mode,
            effect_color: [0.25, 0.5, 0.75, 1.],
        };
        draw.distance_field = Some(style);
        assert!(validate_draw(&draw, None).is_ok());
        let meta = encode_metadata(&draw);
        assert_eq!(meta.distance_mode, code);
        assert_eq!([meta.distance_spread, meta.distance_radius], [8., 2.]);
        assert_eq!(
            [meta.effect_r, meta.effect_g, meta.effect_b, meta.effect_a],
            style.effect_color
        );
        draw.distance_field.as_mut().unwrap().spread_px = f32::NAN;
        assert!(validate_draw(&draw, None).is_err());
    }
    draw.distance_field = None;
    assert_eq!(encode_metadata(&draw).distance_mode, 0);
}

#[test]
fn distance_copy_out_preserves_style_and_atlas_bytes() {
    use worldrunner_world_simulation::{
        presentation::CoverageTexture,
        ui::presentation::{UiDistanceMode, UiDistanceStyle, UiPresentation},
    };
    let mut draw = snapshot_draw(0, 0);
    draw.kind = UiDrawKind::Text;
    draw.texture = Some(UiTexture::Coverage(7));
    draw.corners = [[0., 0., 0.], [16., 0., 0.], [16., 16., 0.], [0., 16., 0.]];
    draw.uv_rect = [0., 0., 1., 1.];
    draw.distance_field = Some(UiDistanceStyle {
        mode: UiDistanceMode::Outline,
        spread_px: 5.,
        radius_px: 2.,
        effect_color: [0.25, 0.5, 0.75, 1.],
    });
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    crate::lock_store()
        .session_mut(world)
        .unwrap()
        .ui_presentation = Some(UiPresentation {
        draws: vec![draw],
        coverage: std::collections::BTreeMap::from([(
            7,
            CoverageTexture {
                width: 2,
                height: 2,
                bytes: std::sync::Arc::from([0, 125, 130, 255]),
            },
        )]),
        ..Default::default()
    });
    let mut meta = DisunityUiDraw::default();
    let mut quad = DisunityQuad::default();
    let mut coverage = DisunityCoverage::default();
    let mut bytes = [99; 4];
    unsafe {
        assert_eq!(
            disunity_presentation_ui_draw(world, 0, &mut meta, &mut quad).code,
            0
        );
        assert_eq!(
            disunity_presentation_ui_coverage(world, 0, &mut coverage, bytes.as_mut_ptr(), 4).code,
            0
        );
    }
    assert_eq!(meta.distance_mode, 2);
    assert_eq!([meta.distance_spread, meta.distance_radius], [5., 2.]);
    assert_eq!(
        [meta.effect_r, meta.effect_g, meta.effect_b, meta.effect_a],
        [0.25, 0.5, 0.75, 1.]
    );
    assert_eq!(quad.texture, 7);
    assert_eq!(bytes, [0, 125, 130, 255]);
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}

#[test]
fn a_draw_with_a_material_round_trips_the_abi() {
    use worldrunner_world_simulation::{
        graphics::MaterialId,
        ui::presentation::{UiMaterial, UiPresentation},
    };
    let material = UiMaterial {
        id: MaterialId::from_bits((3 << 32) | 5),
        generation: 9,
    };
    let (mut shaped, plain) = (snapshot_draw(0, 0), snapshot_draw(0, 1));
    shaped.material = Some(material);
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    crate::lock_store()
        .session_mut(world)
        .unwrap()
        .ui_presentation = Some(UiPresentation {
        draws: vec![shaped, plain],
        ..Default::default()
    });
    let read = |index| {
        let (mut meta, mut quad) = (DisunityUiDraw::default(), DisunityQuad::default());
        let result = unsafe { disunity_presentation_ui_draw(world, index, &mut meta, &mut quad) };
        assert_eq!(result.code, 0);
        (meta.material, meta.material_generation)
    };
    assert_eq!(read(0), (material.id.to_bits(), 9));
    assert_eq!(
        read(1),
        (0, 0),
        "a draw with the engine's look names no material"
    );
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}

#[test]
fn a_target_draw_round_trips_the_abi() {
    let target = RenderTextureId::from_bits((3 << 32) | 5);
    let mut draw = snapshot_draw(0, 0);
    draw.texture = Some(UiTexture::Target(target));
    assert!(validate_draw(&draw, None).is_ok());
    let (meta, quad) = (encode_metadata(&draw), encode(&draw));
    assert_eq!(meta.texture_kind, 1);
    assert_eq!((quad.texture, quad.textured), (target.to_bits(), 1));
    draw.texture = Some(UiTexture::Image(1 << 40));
    let (meta, quad) = (encode_metadata(&draw), encode(&draw));
    assert_eq!(meta.texture_kind, 0);
    assert_eq!((quad.texture, quad.textured), (1 << 40, 1));
}

#[test]
fn a_target_draw_on_a_text_quad_is_refused() {
    let mut draw = snapshot_draw(0, 0);
    draw.kind = UiDrawKind::Text;
    draw.texture = Some(UiTexture::Target(RenderTextureId::from_bits(5)));
    assert!(validate_draw(&draw, None).is_err());
}
