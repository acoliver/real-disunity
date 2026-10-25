use crate::*;

fn tone() -> ToneCurveBright {
    ToneCurveBright {
        curve: BrightCurve::AcesFilmic,
        maximum: 32.0,
        luminance: [0.2126, 0.7152, 0.0722],
        exposure_scale: 1.0,
        contrast: 1.0,
        epsilon: 0.00001,
    }
}

#[test]
fn tone_policy_validates_all_authored_scalars() {
    assert!(tone().validate().is_ok());
    for invalid in [f32::NAN, f32::INFINITY, -1.0] {
        for policy in [
            ToneCurveBright {
                maximum: invalid,
                ..tone()
            },
            ToneCurveBright {
                epsilon: invalid,
                ..tone()
            },
            ToneCurveBright {
                exposure_scale: invalid,
                ..tone()
            },
            ToneCurveBright {
                contrast: invalid,
                ..tone()
            },
            ToneCurveBright {
                luminance: [invalid, 0.0, 1.0],
                ..tone()
            },
        ] {
            assert!(policy.validate().is_err());
        }
    }
}

#[test]
fn tone_policy_rejects_zero_and_out_of_range_values() {
    for invalid in [0.0, 65505.0] {
        assert!(
            ToneCurveBright {
                maximum: invalid,
                ..tone()
            }
            .validate()
            .is_err()
        );
        assert!(
            ToneCurveBright {
                epsilon: invalid,
                ..tone()
            }
            .validate()
            .is_err()
        );
    }
    for invalid in [65505.0, f32::MAX] {
        assert!(
            ToneCurveBright {
                contrast: invalid,
                ..tone()
            }
            .validate()
            .is_err()
        );
        assert!(
            ToneCurveBright {
                exposure_scale: invalid,
                ..tone()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        ToneCurveBright {
            luminance: [0.0; 3],
            ..tone()
        }
        .validate()
        .is_err()
    );
    assert!(
        ToneCurveBright {
            luminance: [1.1, 0.0, 0.0],
            ..tone()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn level_selection_rejects_inaccessible_outputs() {
    for selection in [
        BloomLevelSelection {
            first_blurred: 0,
            output: 0,
        },
        BloomLevelSelection {
            first_blurred: 1,
            output: 1,
        },
        BloomLevelSelection {
            first_blurred: 1,
            output: 5,
        },
    ] {
        assert!(selection.validate(6).is_ok());
    }
    for (first_blurred, output, count) in [(6, 1, 6), (1, 0, 6), (0, 6, 6), (0, 0, 0), (0, 0, 9)] {
        assert!(
            BloomLevelSelection {
                first_blurred,
                output
            }
            .validate(count)
            .is_err()
        );
    }
}

#[test]
fn every_policy_combination_validates_and_tent_rejects_changes() {
    for curve in [BrightCurve::AcesFilmic, BrightCurve::Reinhard] {
        for reduction in [
            BloomReduction::BoxPerTexel,
            BloomReduction::QuarterTexelAfterAverage,
        ] {
            for kernel in [
                GaussianKernelRule::RadiusThird,
                GaussianKernelRule::AxisClampedPairs,
            ] {
                for tint_pass in [BloomTintPass::Horizontal, BloomTintPass::Vertical] {
                    let policies = BloomPolicies {
                        extraction: BrightExtraction::ToneCurve(ToneCurveBright {
                            curve,
                            ..tone()
                        }),
                        reduction,
                        kernel,
                        tint_pass,
                        selection: BloomLevelSelection::default(),
                        terminal: BloomTerminal::default(),
                    };
                    assert!(policies.validate(2).is_ok());
                    let bloom = Bloom {
                        policies,
                        threshold: 1.0,
                        knee: 0.5,
                        intensity: 1.0,
                        levels: 2,
                    };
                    let settings = HdrPostSettings {
                        bloom: Some(bloom),
                        ..HdrPostSettings::default()
                    };
                    assert!(settings.validate().is_err());
                    let level = GaussianLevel {
                        radius: 4,
                        tint: [1.0; 3],
                        weight: 1.0,
                    };
                    assert!(
                        HdrPostSettings {
                            bloom_kernel: BloomKernel::SeparablePyramid([level; 8]),
                            ..settings
                        }
                        .validate()
                        .is_ok()
                    );
                }
            }
        }
    }
}

fn pyramid_settings(extraction: BrightExtraction, threshold: f32) -> HdrPostSettings {
    let level = GaussianLevel {
        radius: 4,
        tint: [1.0; 3],
        weight: 1.0,
    };
    HdrPostSettings {
        bloom: Some(Bloom {
            policies: BloomPolicies {
                extraction,
                ..BloomPolicies::default()
            },
            threshold,
            knee: 0.5,
            intensity: 1.0,
            levels: 2,
        }),
        bloom_kernel: BloomKernel::SeparablePyramid([level; 8]),
        ..HdrPostSettings::default()
    }
}

#[test]
fn tone_curve_scalars_accept_the_half_float_range() {
    for value in [64.5, 100.0, 1000.0, 10_000.0, 65504.0] {
        for policy in [
            ToneCurveBright {
                contrast: value,
                ..tone()
            },
            ToneCurveBright {
                exposure_scale: value,
                ..tone()
            },
        ] {
            assert!(policy.validate().is_ok(), "{value}");
        }
    }
}

#[test]
fn bloom_threshold_limit_follows_extraction_policy() {
    let tone_curve = BrightExtraction::ToneCurve(ToneCurveBright {
        contrast: 10_000.0,
        ..tone()
    });
    for threshold in [0.0, 64.0, 65.0, 100.0, 65504.0] {
        assert!(
            pyramid_settings(tone_curve, threshold).validate().is_ok(),
            "{threshold}"
        );
    }
    for threshold in [-1.0, 65505.0, f32::NAN, f32::INFINITY] {
        assert!(
            pyramid_settings(tone_curve, threshold).validate().is_err(),
            "{threshold}"
        );
    }
    assert!(
        pyramid_settings(BrightExtraction::SoftKnee, 64.0)
            .validate()
            .is_ok()
    );
    for threshold in [65.0, 100.0] {
        assert!(
            pyramid_settings(BrightExtraction::SoftKnee, threshold)
                .validate()
                .is_err(),
            "{threshold}"
        );
    }
}
