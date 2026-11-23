use super::*;
use worldrunner_world_simulation::{graphics::RenderTextureId, ui::UiTexture};

#[test]
fn triangle_copy_out_preserves_uvs_and_three_corner_topology() {
    for depth_test in [false, true] {
        triangle_copy_out(depth_test);
    }
}

fn triangle_copy_out(depth_test: bool) {
    let meta = native::DisunityUiDraw {
        kind: 2 | (u32::from(depth_test) << 2),
        triangle_u0: 0.1,
        triangle_v0: 0.2,
        triangle_u1: 0.3,
        triangle_v1: 0.4,
        triangle_u2: 0.5,
        triangle_v2: 0.6,
        ..Default::default()
    };
    let quad = native::DisunityQuad {
        overlay: 1,
        p0_x: 10.,
        p1_x: 20.,
        p2_y: 30.,
        ..Default::default()
    };
    let draw = decode(meta, quad).unwrap();
    assert_eq!(draw.kind, UiDrawKind::Graphic);
    assert_eq!(draw.depth_test, depth_test);
    assert_eq!(draw.triangle_uv, Some([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]));
    assert_eq!(
        draw.corners[..3],
        [[10., 0., 0.], [20., 0., 0.], [0., 30., 0.]]
    );
}

#[test]
fn ui_decode_preserves_signed_order_hierarchy_and_clip_for_both_kinds() {
    for (kind, expected) in [(0, UiDrawKind::Graphic), (1, UiDrawKind::Text)] {
        let meta = native::DisunityUiDraw {
            kind,
            hierarchy_order: 17,
            clip_x: 12,
            clip_y: 24,
            clip_width: 8,
            clip_height: 16,
            ..Default::default()
        };
        let quad = native::DisunityQuad {
            overlay: 1,
            sorting_layer: -3,
            sorting_order: 7,
            p0_x: 12.,
            p0_y: 24.,
            ..Default::default()
        };
        let draw = decode(meta, quad).unwrap();
        assert_eq!(draw.kind, expected);
        assert_eq!(draw.triangle_uv, None);
        assert_eq!(
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order),
            (-3, 7, 17)
        );
        assert_eq!(draw.clip_rect, [12, 24, 8, 16]);
        assert_eq!(draw.corners[0], [12., 24., 0.]);
    }
}

#[test]
fn ui_decode_rejects_unknown_draw_kind() {
    let meta = native::DisunityUiDraw {
        kind: 3,
        ..Default::default()
    };
    assert!(matches!(decode(meta, native::DisunityQuad::default()),
        Err(HostError::Managed(message)) if message.contains("unknown UI draw kind 3")));
}

#[test]
fn corner_metadata_decode_preserves_order_and_common_tint() {
    let meta = corner_metadata();
    let quad = native::DisunityQuad {
        overlay: 1,
        r: 0.25,
        a: 0.5,
        ..Default::default()
    };
    let draw = decode(meta, quad).unwrap();
    assert_eq!(
        draw.corner_colors,
        Some([
            [1., 0., 0., 1.],
            [0., 1., 0., 0.75],
            [0., 0., 1., 0.5],
            [1., 1., 1., 0.25],
        ])
    );
    assert_eq!(draw.color, [0.25, 0., 0., 0.5]);
    let draw = decode(native::DisunityUiDraw::default(), quad).unwrap();
    assert_eq!(draw.corner_colors, None);
}

fn corner_metadata() -> native::DisunityUiDraw {
    native::DisunityUiDraw {
        corner_colors_present: 1,
        corner_0_r: 1.,
        corner_0_a: 1.,
        corner_1_g: 1.,
        corner_1_a: 0.75,
        corner_2_b: 1.,
        corner_2_a: 0.5,
        corner_3_r: 1.,
        corner_3_g: 1.,
        corner_3_b: 1.,
        corner_3_a: 0.25,
        ..Default::default()
    }
}

#[test]
fn corner_decode_rejects_invalid_channels_presence_and_non_overlay_metadata() {
    let quad = native::DisunityQuad {
        overlay: 1,
        ..Default::default()
    };
    for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        let mut meta = corner_metadata();
        meta.corner_3_a = value;
        assert!(
            matches!(decode(meta, quad), Err(HostError::Managed(message))
            if message == "invalid UI corner_colors")
        );
    }
    let mut meta = corner_metadata();
    meta.corner_colors_present = 2;
    assert!(decode(meta, quad).is_err());
    assert!(
        matches!(decode(corner_metadata(), native::DisunityQuad::default()),
        Err(HostError::Managed(message)) if message.contains("non-overlay quad"))
    );
}

#[test]
fn distance_metadata_decodes_all_modes_and_rejects_invalid_records() {
    use worldrunner_world_simulation::ui::presentation::UiDistanceMode;
    let quad = native::DisunityQuad {
        overlay: 1,
        textured: 1,
        texture: 7,
        p1_x: 16.,
        p2_x: 16.,
        p2_y: 16.,
        p3_y: 16.,
        u1: 1.,
        v1: 1.,
        ..Default::default()
    };
    let mut meta = native::DisunityUiDraw {
        kind: 1,
        distance_spread: 8.,
        distance_radius: 2.,
        effect_r: 0.25,
        effect_g: 0.5,
        effect_b: 0.75,
        effect_a: 1.,
        ..Default::default()
    };
    for (code, mode) in [
        (1, UiDistanceMode::Fill),
        (2, UiDistanceMode::Outline),
        (3, UiDistanceMode::Glow),
    ] {
        meta.distance_mode = code;
        let style = decode(meta, quad).unwrap().distance_field.unwrap();
        assert_eq!(style.mode, mode);
        assert_eq!([style.spread_px, style.radius_px], [8., 2.]);
        assert_eq!(style.effect_color, [0.25, 0.5, 0.75, 1.]);
    }
    meta.distance_mode = 4;
    assert!(decode(meta, quad).is_err());
    meta.distance_mode = 2;
    meta.distance_spread = 0.;
    assert!(decode(meta, quad).is_err());
    meta.distance_spread = 8.;
    meta.kind = 0;
    assert!(decode(meta, quad).is_err());
    meta.kind = 1;
    meta.distance_radius = -1.;
    assert!(decode(meta, quad).is_err());
}

fn textured(
    texture_kind: u32,
    kind: u32,
    texture: u64,
) -> (native::DisunityUiDraw, native::DisunityQuad) {
    let meta = native::DisunityUiDraw {
        kind,
        texture_kind,
        ..Default::default()
    };
    let quad = native::DisunityQuad {
        overlay: 1,
        textured: 1,
        texture,
        ..Default::default()
    };
    (meta, quad)
}

#[test]
fn a_render_texture_draw_decodes_with_its_target_id() {
    let target = RenderTextureId::from_bits((2 << 32) | 9);
    let (meta, quad) = textured(1, 0, target.to_bits());
    let draw = decode(meta, quad).unwrap();
    assert_eq!(draw.texture, Some(UiTexture::Target(target)));
}

#[test]
fn image_and_coverage_draws_decode_by_their_draw_kind() {
    let (meta, quad) = textured(0, 0, 1 << 40);
    assert_eq!(
        decode(meta, quad).unwrap().texture,
        Some(UiTexture::Image(1 << 40))
    );
    let (meta, quad) = textured(0, 1, 3);
    assert_eq!(
        decode(meta, quad).unwrap().texture,
        Some(UiTexture::Coverage(3))
    );
}

#[test]
fn a_render_texture_on_text_or_an_unknown_texture_kind_is_rejected() {
    let (meta, quad) = textured(1, 1, 5);
    assert!(decode(meta, quad).is_err());
    let (meta, quad) = textured(2, 0, 5);
    assert!(matches!(decode(meta, quad),
        Err(HostError::Managed(message)) if message.contains("texture kind 2")));
}

#[test]
fn a_material_identity_decodes_and_a_generation_without_one_is_rejected() {
    let quad = native::DisunityQuad {
        overlay: 1,
        ..Default::default()
    };
    let plain = decode(native::DisunityUiDraw::default(), quad).unwrap();
    assert_eq!(plain.material, None);
    let shaped = native::DisunityUiDraw {
        material: (2 << 32) | 6,
        material_generation: 4,
        ..Default::default()
    };
    let material = decode(shaped, quad).unwrap().material.unwrap();
    assert_eq!(material.id.to_bits(), (2 << 32) | 6);
    assert_eq!(material.generation, 4);
    let orphan = native::DisunityUiDraw {
        material_generation: 4,
        ..Default::default()
    };
    assert!(decode(orphan, quad).is_err());
}
