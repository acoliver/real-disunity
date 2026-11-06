use super::*;

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
        kind: 2,
        ..Default::default()
    };
    assert!(matches!(decode(meta, native::DisunityQuad::default()),
        Err(HostError::Managed(message)) if message.contains("unknown UI draw kind 2")));
}
