use crate::{ColorGrading, HdrPostSettings, ToneMap, Vignette};

#[test]
fn matrix_fit_ramp_primaries_and_special_values() {
    let vectors = [
        ([0.0; 3], [0.0; 3]),
        (
            [0.5; 3],
            [
                0.374_308_314_016_463_53,
                0.374_308_314_016_463_6,
                0.374_304_570_933_323_37,
            ],
        ),
        (
            [1.0; 3],
            [
                0.619_115_426_940_809,
                0.619_115_426_940_809,
                0.619_109_235_786_539_6,
            ],
        ),
        (
            [4.0, 0.0, 0.0],
            [1.0, 0.154_312_698_885_234_38, 0.037_482_015_889_969_7],
        ),
    ];
    for (input, expected) in vectors {
        let actual = ToneMap::AcesFitted.evaluate_rgb(input, 1.0);
        for (a, e) in actual.into_iter().zip(expected) {
            assert!((a - e).abs() < 1e-10, "{input:?}: {actual:?}");
        }
    }
    for i in 0..3 {
        let mut rgb = [0.0; 3];
        rgb[i] = 4.0;
        assert!(
            ToneMap::AcesFitted
                .evaluate_rgb(rgb, 1.0)
                .into_iter()
                .zip(rgb.map(|v| ToneMap::Filmic.evaluate(v, 1.0)))
                .any(|(a, b)| (a - b).abs() > 0.01)
        );
    }
    for value in [-1.0, f64::NEG_INFINITY, f64::NAN] {
        assert_rgb(ToneMap::AcesFitted.evaluate_rgb([value; 3], 1.0), [0.0; 3]);
    }
    assert_rgb(
        ToneMap::AcesFitted.evaluate_rgb([f64::INFINITY; 3], 1.0),
        [1.0; 3],
    );
    assert_rgb(
        ToneMap::AcesFitted.evaluate_rgb([f64::MAX; 3], 4.0),
        [1.0; 3],
    );
    assert_rgb(
        ToneMap::AcesFitted.evaluate_rgb([f64::INFINITY; 3], 0.0),
        [0.0; 3],
    );
}

#[test]
fn composition_and_display_stages_have_explicit_order() {
    let grading = ColorGrading {
        linear_scale: 2.0,
        display_power: [2.0, 1.0, 0.5],
        vignette: None,
    };
    let actual = grading.evaluate(
        ToneMap::AcesFitted,
        [0.5; 3],
        0.5,
        ([0.25; 3], 0.5),
        [0.5; 2],
    );
    let curve = ToneMap::AcesFitted.evaluate_rgb([0.75; 3], 1.0);
    assert_rgb(actual, [curve[0].powi(2), curve[1], curve[2].sqrt()]);
    let neutral = ColorGrading::default();
    for curve in [ToneMap::Reinhard, ToneMap::Filmic, ToneMap::AcesFitted] {
        let rgb = curve.evaluate_rgb([0.125, 0.5, 4.0], 1.0);
        assert_eq!(
            neutral.apply(rgb, [0.1, 0.9]).map(f64::to_bits),
            rgb.map(f64::to_bits)
        );
    }
    assert!(!HdrPostSettings::default().enabled());
}

#[test]
fn legacy_reference_preserves_composition_and_special_values() {
    let grading = ColorGrading::default();
    for curve in [ToneMap::Reinhard, ToneMap::Filmic] {
        for exposure in [0.0, 0.5, 2.0] {
            for value in [0.5, -0.5, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
                let expected = if value.is_finite() {
                    curve.evaluate(value.max(0.0) + 0.25 * 0.5, exposure)
                } else {
                    curve.evaluate(value, exposure)
                };
                assert_rgb(
                    grading.evaluate(curve, [value; 3], exposure, ([0.25; 3], 0.5), [0.5; 2]),
                    [expected; 3],
                );
            }
        }
        let graded = ColorGrading {
            linear_scale: 2.0,
            ..grading
        };
        assert_rgb(
            graded.evaluate(curve, [0.5; 3], 0.5, ([0.25; 3], 0.5), [0.5; 2]),
            curve.evaluate_rgb([0.75; 3], 1.0),
        );
    }
}

#[test]
fn positive_infinity_survives_small_positive_multipliers() {
    for exposure in [0.0, 1e-18, 1.0] {
        for scale in [0.0, 1e-18, 1.0] {
            let grading = ColorGrading {
                linear_scale: scale,
                ..Default::default()
            };
            let expected = if exposure == 0.0 || scale == 0.0 {
                [0.0; 3]
            } else {
                [1.0; 3]
            };
            assert_rgb(
                grading.evaluate(
                    ToneMap::AcesFitted,
                    [f64::INFINITY; 3],
                    exposure,
                    ([0.0; 3], 0.0),
                    [0.5; 2],
                ),
                expected,
            );
        }
    }
}

#[test]
fn vignette_pixel_centres_edges_corners_and_validation() {
    let vignette = Vignette {
        axis_scale: [1.0; 2],
        tint_range: [0.0, 1.0],
        tint: [0.2, 0.4, 0.6, 0.5],
        darken_range: [1.0, 0.0],
        darken_opacity: 0.75,
    };
    let grading = ColorGrading {
        vignette: Some(vignette),
        ..Default::default()
    };
    assert_rgb(grading.apply([0.5; 3], [0.5; 2]), [0.5; 3]);
    for pixel in [[2.0_f64, 0.0], [0.0, 0.0]] {
        let uv = pixel.map(|p| (p + 0.5) / 5.0);
        let radius = (uv[0] - 0.5).hypot(uv[1] - 0.5) / 0.5_f64.sqrt();
        let t = radius * radius * (3.0 - 2.0 * radius);
        let expected: [f64; 3] = std::array::from_fn(|i| {
            (0.5 + (f64::from(vignette.tint[i]) - 0.5) * t * 0.5) * (1.0 - t * 0.75)
        });
        for (a, e) in grading.apply([0.5; 3], uv).into_iter().zip(expected) {
            assert!((a - e).abs() < 1e-7);
        }
    }
    for bad in [f32::NAN, f32::INFINITY, -1.0, 65.0] {
        assert!(
            ColorGrading {
                linear_scale: bad,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    for bad in [0.0, f32::NAN, f32::INFINITY, 17.0] {
        assert!(
            ColorGrading {
                display_power: [bad; 3],
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        ColorGrading {
            vignette: Some(Vignette {
                tint_range: [0.5; 2],
                ..vignette
            }),
            ..grading
        }
        .validate()
        .is_err()
    );
    assert!(grading.validate().is_ok());
}

fn assert_rgb(actual: [f64; 3], expected: [f64; 3]) {
    assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
}
