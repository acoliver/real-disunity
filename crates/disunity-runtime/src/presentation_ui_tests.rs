use super::*;

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
