use super::validation::validate_quad;
use super::{DynamicError, DynamicQuad};

#[test]
fn overlay_metadata_on_scene_quads_is_rejected() {
    let quad = DynamicQuad {
        corner_colors: Some([[1.; 4]; 4]),
        ..DynamicQuad::default()
    };
    assert!(matches!(
        validate_quad(&quad, 3, 0),
        Err(DynamicError::InvalidQuad {
            index: 3,
            field: "overlay metadata"
        })
    ));
}

#[test]
fn corner_colors_reject_every_invalid_channel_and_accept_endpoints() {
    let mut quad = DynamicQuad {
        overlay: true,
        ..DynamicQuad::default()
    };
    for invalid in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
        for corner in 0..4 {
            for channel in 0..4 {
                let mut colors = [[1.; 4]; 4];
                colors[corner][channel] = invalid;
                quad.corner_colors = Some(colors);
                assert!(matches!(
                    validate_quad(&quad, 2, 0),
                    Err(DynamicError::InvalidQuad {
                        index: 2,
                        field: "corner_colors"
                    })
                ));
            }
        }
    }
    for colors in [None, Some([[0.; 4]; 4]), Some([[1.; 4]; 4])] {
        quad.corner_colors = colors;
        assert!(validate_quad(&quad, 0, 0).is_ok());
    }
}

#[test]
fn presentation_conversion_preserves_and_validates_corner_metadata() {
    use std::collections::BTreeMap;
    use worldrunner_render_plan::ui::{UiDraw, UiDrawKind, UiPresentation};
    let colors = [[0.25; 4], [0.5; 4], [0.75; 4], [1.; 4]];
    let mut plan = UiPresentation {
        draws: vec![UiDraw {
            kind: UiDrawKind::Graphic,
            depth_test: false,
            triangle_uv: None,
            corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
            uv_rect: [0., 0., 1., 1.],
            color: [0.5; 4],
            corner_colors: Some(colors),
            texture: None,
            sorting_layer: 0,
            sorting_order: 0,
            hierarchy_order: 0,
            clip_rect: [0, 0, 64, 64],
        }],
        ..Default::default()
    };
    let result = crate::simulation::UiOverlay::from_presentation(&plan, &BTreeMap::new()).unwrap();
    assert_eq!(result.quads[0].corner_colors, Some(colors));
    assert_eq!(
        result.quads[0].tint.map(f32::to_bits),
        [0.5f32.to_bits(); 4]
    );
    for value in [f32::NAN, -0.1, 1.1] {
        plan.draws[0].corner_colors.as_mut().unwrap()[2][3] = value;
        assert!(matches!(
            crate::simulation::UiOverlay::from_presentation(&plan, &BTreeMap::new()),
            Err(DynamicError::InvalidQuad {
                index: 0,
                field: "corner_colors"
            })
        ));
    }
}
