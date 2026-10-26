use super::*;
use crate::{SceneBloom, SceneHdrPost};

fn recipe() -> SceneBloomRecipe {
    SceneBloomRecipe {
        extraction: SceneBrightExtraction::ToneCurve(SceneToneCurveBright {
            curve: SceneBrightCurve::AcesFilmic,
            maximum_micro: 65_504_000_000,
            luminance_micro: [250_000, 500_000, 250_000],
            exposure_scale_micro: 1_000_000,
            contrast_micro: 1_000_000,
            epsilon_micro: 1,
        }),
        reduction: SceneBloomReduction::QuarterTexelAfterAverage,
        kernel: SceneGaussianKernelRule::AxisClampedPairs,
        selection: SceneBloomLevelSelection {
            first_blurred: 1,
            output: 2,
        },
        tint_pass: SceneBloomTintPass::Horizontal,
        terminal: SceneBloomTerminal::Repeat,
        levels: [SceneGaussianLevel {
            radius: 5,
            tint_micro: [1_000_000; 3],
            weight_micro: 500_000,
        }; 8],
    }
}

fn post(r: SceneBloomRecipe) -> SceneHdrPost {
    SceneHdrPost {
        tone_map: None,
        grading: None,
        adaptive_exposure: None,
        bloom: Some(SceneBloom {
            threshold_micro: 500_000,
            soft_knee_micro: 500_000,
            intensity_micro: 500_000,
            levels: 3,
            recipe: Some(r),
        }),
        fxaa_subpixel_micro: None,
    }
}

#[test]
fn bloom_recipe_round_trip_and_omitted_bytes() {
    let p = post(recipe());
    p.validate().unwrap();
    let bytes = serde_json::to_vec(&p).unwrap();
    let decoded: SceneHdrPost = serde_json::from_slice(&bytes).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded, p);
    let old = r#"{"threshold_micro":500000,"soft_knee_micro":500000,"intensity_micro":500000,"levels":3}"#;
    let b: SceneBloom = serde_json::from_str(old).unwrap();
    assert_eq!(b.recipe, None);
    assert_eq!(serde_json::to_string(&b).unwrap(), old);
}

#[test]
fn invalid_recipe_quantities_and_selections_fail() {
    let r = recipe();
    for count in [0, 1, 2, 9] {
        assert!(r.validate(count).is_err());
    }
    for selection in [
        SceneBloomLevelSelection {
            first_blurred: 3,
            output: 3,
        },
        SceneBloomLevelSelection {
            first_blurred: 2,
            output: 1,
        },
        SceneBloomLevelSelection {
            first_blurred: 0,
            output: 3,
        },
    ] {
        assert!(
            post(SceneBloomRecipe { selection, ..r })
                .validate()
                .is_err()
        );
    }
    for level in [
        SceneGaussianLevel {
            radius: 65,
            ..r.levels[0]
        },
        SceneGaussianLevel {
            tint_micro: [64_000_001; 3],
            ..r.levels[0]
        },
        SceneGaussianLevel {
            weight_micro: 64_000_001,
            ..r.levels[0]
        },
    ] {
        let mut invalid = r;
        invalid.levels[7] = level;
        assert!(post(invalid).validate().is_err());
    }
}

#[test]
fn invalid_tone_curve_quantities_fail() {
    let r = recipe();
    let SceneBrightExtraction::ToneCurve(c) = r.extraction else {
        panic!()
    };
    for c in [
        SceneToneCurveBright {
            maximum_micro: 0,
            ..c
        },
        SceneToneCurveBright {
            epsilon_micro: 0,
            ..c
        },
        SceneToneCurveBright {
            contrast_micro: 65_504_000_001,
            ..c
        },
        SceneToneCurveBright {
            exposure_scale_micro: u64::MAX,
            ..c
        },
        SceneToneCurveBright {
            luminance_micro: [u32::MAX; 3],
            ..c
        },
    ] {
        assert!(
            post(SceneBloomRecipe {
                extraction: SceneBrightExtraction::ToneCurve(c),
                ..r
            })
            .validate()
            .is_err()
        );
    }
}

#[test]
fn unknown_fields_and_noninteger_tints_are_rejected() {
    let mut json = serde_json::to_value(recipe()).unwrap();
    json["extra"] = true.into();
    assert!(serde_json::from_value::<SceneBloomRecipe>(json.clone()).is_err());
    json.as_object_mut().unwrap().remove("extra");
    json["selection"]["extra"] = true.into();
    assert!(serde_json::from_value::<SceneBloomRecipe>(json.clone()).is_err());
    json["selection"].as_object_mut().unwrap().remove("extra");
    json["levels"][0]["tint_micro"][0] = serde_json::json!(0.5);
    assert!(serde_json::from_value::<SceneBloomRecipe>(json).is_err());
}
