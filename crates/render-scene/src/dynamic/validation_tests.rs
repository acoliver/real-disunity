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
