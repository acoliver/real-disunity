use super::{
    SceneAntiAlias, SceneSmaaEdgeDetection, SceneSmaaPreset, SceneSmaaSettings,
    SceneValidationError,
};

#[test]
fn smaa_presets_accept_both_edge_modes() {
    for (preset, expected) in [
        (SceneSmaaPreset::Low, SceneSmaaSettings::LOW),
        (SceneSmaaPreset::Medium, SceneSmaaSettings::MEDIUM),
        (SceneSmaaPreset::High, SceneSmaaSettings::HIGH),
        (SceneSmaaPreset::Ultra, SceneSmaaSettings::ULTRA),
    ] {
        assert_eq!(SceneAntiAlias::smaa(preset), SceneAntiAlias::Smaa(expected));
        for edge_detection in [SceneSmaaEdgeDetection::Luma, SceneSmaaEdgeDetection::Color] {
            assert_eq!(
                SceneAntiAlias::Smaa(SceneSmaaSettings {
                    edge_detection,
                    ..expected
                })
                .validate(),
                Ok(())
            );
        }
    }
}

#[test]
fn smaa_custom_settings_accept_inclusive_bounds_and_distinguish_off_from_zero() {
    for threshold_micro in [1, 500_000] {
        for max_search_steps in [0, 112] {
            for max_search_steps_diag in [None, Some(0), Some(20)] {
                for corner_rounding in [None, Some(0), Some(100)] {
                    let settings = SceneSmaaSettings {
                        threshold_micro,
                        max_search_steps,
                        max_search_steps_diag,
                        corner_rounding,
                        edge_detection: SceneSmaaEdgeDetection::Color,
                    };
                    assert_eq!(settings.validate(), Ok(()));
                    assert_eq!(SceneAntiAlias::Smaa(settings).validate(), Ok(()));
                }
            }
        }
    }
}

#[test]
fn smaa_out_of_range_fields_return_exact_typed_errors() {
    let base = SceneSmaaSettings::ULTRA;
    let mut cases = Vec::new();
    for observed in [0, 500_001, u32::MAX] {
        cases.push((
            SceneSmaaSettings {
                threshold_micro: observed,
                ..base
            },
            "SMAA threshold_micro",
            observed,
            1,
            500_000,
        ));
    }
    for observed in [113, u32::MAX] {
        cases.push((
            SceneSmaaSettings {
                max_search_steps: observed,
                ..base
            },
            "SMAA max_search_steps",
            observed,
            0,
            112,
        ));
    }
    for observed in [21, u32::MAX] {
        cases.push((
            SceneSmaaSettings {
                max_search_steps_diag: Some(observed),
                ..base
            },
            "SMAA max_search_steps_diag",
            observed,
            0,
            20,
        ));
    }
    for observed in [101, u32::MAX] {
        cases.push((
            SceneSmaaSettings {
                corner_rounding: Some(observed),
                ..base
            },
            "SMAA corner_rounding",
            observed,
            0,
            100,
        ));
    }
    for (settings, part, observed, minimum, maximum) in cases {
        let expected = Err(SceneValidationError::RangeViolation {
            part,
            minimum,
            maximum,
            observed: u64::from(observed),
        });
        assert_eq!(settings.validate(), expected);
        assert_eq!(SceneAntiAlias::Smaa(settings).validate(), expected);
    }
}
