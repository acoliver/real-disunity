//! Affine closure of public immediate-text placements after f32 pixel rounding.
#![cfg(feature = "ui")]
mod ui_font;
use glam::UVec2;
use worldrunner_world_simulation::ui::presentation::validate_distance_mapping;
use worldrunner_world_simulation::ui::*;

fn effects() -> [UiTextEffect; 2] {
    [
        UiTextEffect::Outline {
            color: [1.; 4],
            size: 2.,
        },
        UiTextEffect::Glow {
            color: [1.; 4],
            size: 2.,
        },
    ]
}

fn fixture() -> (FontBook, TextRun) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut run = TextRun::new(fonts.register(ui_font::font()).unwrap());
    run.text = "AA".into();
    run.font_size = 20.;
    (fonts, run)
}

fn assert_placement(fonts: &FontBook, run: &TextRun, viewport: u32, x: f32, y: f32, angle: f32) {
    for effect in effects() {
        let mut list = UiDrawList::new(
            UVec2::splat(viewport),
            fonts,
            UiOrder::default(),
            UiDrawLimits::default(),
        )
        .unwrap();
        let placement = UiTextPlacement {
            rotation_radians: angle,
            ..UiTextPlacement::new(UiRect::new(x, y, 50., 40.))
        };
        assert_eq!(
            list.text(run, placement, effect),
            Ok(()),
            "viewport={viewport}, origin=({x}, {y}), angle={angle}, effect={effect:?}"
        );
        assert_eq!(list.finish().unwrap().draws.len(), 2);
    }
}

#[test]
fn distance_rotation_accepts_review_placement_for_outline_and_glow() {
    let (fonts, run) = fixture();
    assert_placement(&fonts, &run, 16384, 100., 4096., 0.47);
}

#[test]
fn distance_rotation_accepts_in_viewport_placement_sweep() {
    let (fonts, run) = fixture();
    for viewport in [1024_u16, 2048, 4096, 8192, 16384] {
        let size = f32::from(viewport);
        for x in [100., size * 0.5, size - 200.] {
            for y in [100., size * 0.5, size - 200.] {
                for step in 1..100_u16 {
                    assert_placement(
                        &fonts,
                        &run,
                        u32::from(viewport),
                        x,
                        y,
                        f32::from(step) * 0.01,
                    );
                }
            }
        }
    }
}

#[test]
fn distance_mapping_rejects_non_affine_quads_at_small_and_large_coordinates() {
    for origin in [0., 4096., 16000.] {
        let mut corners = [
            [origin, origin, 0.],
            [origin + 16., origin, 0.],
            [origin + 16., origin + 16., 0.],
            [origin, origin + 16., 0.],
        ];
        assert_eq!(validate_distance_mapping(corners, [0., 0., 1., 1.]), Ok(()));
        corners[2][0] += 0.25;
        assert_eq!(
            validate_distance_mapping(corners, [0., 0., 1., 1.]),
            Err(UiError::InvalidDistanceMapping)
        );
    }
}

#[test]
fn distance_mapping_still_rejects_nonfinite_and_singular_inputs() {
    let corners = [[0., 0., 0.], [16., 0., 0.], [16., 16., 0.], [0., 16., 0.]];
    let uv = [0., 0., 1., 1.];
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut invalid = corners;
        invalid[2][0] = value;
        assert_eq!(
            validate_distance_mapping(invalid, uv),
            Err(UiError::InvalidDistanceMapping)
        );
    }
    assert_eq!(
        validate_distance_mapping(corners, [0.; 4]),
        Err(UiError::InvalidDistanceMapping)
    );
    assert_eq!(
        validate_distance_mapping([[0.; 3]; 4], uv),
        Err(UiError::InvalidDistanceMapping)
    );
}
