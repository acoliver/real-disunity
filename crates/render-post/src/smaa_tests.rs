use crate::{
    AntiAliasing, HdrPostSettings, MsaaSamples, PostError, PostResolve, SmaaEdgeDetection,
    SmaaPreset, SmaaSettings,
};

#[test]
fn presets_match_the_reference_definitions() {
    // SMAA.hlsl: SMAA_PRESET_LOW/MEDIUM/HIGH/ULTRA.
    let expected = [
        (SmaaPreset::Low, 0.15, 4, None, None),
        (SmaaPreset::Medium, 0.1, 8, None, None),
        (SmaaPreset::High, 0.1, 16, Some(8), Some(25)),
        (SmaaPreset::Ultra, 0.05, 32, Some(16), Some(25)),
    ];
    for (preset, threshold, steps, diagonal, corners) in expected {
        let smaa = SmaaSettings::preset(preset);
        assert_eq!(smaa.edge_detection, SmaaEdgeDetection::Luma);
        assert_eq!(smaa.threshold, threshold);
        assert_eq!(smaa.max_search_steps, steps);
        assert_eq!(smaa.max_search_steps_diag, diagonal);
        assert_eq!(smaa.corner_rounding, corners);
        assert_eq!(smaa.validate(), Ok(()));
        assert_eq!(SmaaSettings::from(preset), smaa);
        assert_eq!(PostResolve::smaa(preset), PostResolve::Smaa(smaa));
    }
    assert_eq!(SmaaSettings::default(), SmaaSettings::HIGH);
}

#[test]
fn custom_fields_are_validated() {
    let valid = SmaaSettings::HIGH;
    let invalid = [
        SmaaSettings {
            threshold: 0.0,
            ..valid
        },
        SmaaSettings {
            threshold: -0.1,
            ..valid
        },
        SmaaSettings {
            threshold: 0.51,
            ..valid
        },
        SmaaSettings {
            threshold: f32::NAN,
            ..valid
        },
        SmaaSettings {
            threshold: f32::INFINITY,
            ..valid
        },
        SmaaSettings {
            max_search_steps: 113,
            ..valid
        },
        SmaaSettings {
            max_search_steps_diag: Some(21),
            ..valid
        },
        SmaaSettings {
            corner_rounding: Some(101),
            ..valid
        },
    ];
    for smaa in invalid {
        assert_eq!(
            smaa.validate(),
            Err(PostError::UnsupportedProfile("SMAA settings")),
            "{smaa:?}"
        );
        assert!(PostResolve::Smaa(smaa).validate().is_err());
        assert!(
            AntiAliasing::new(MsaaSamples::X1, PostResolve::Smaa(smaa))
                .validate()
                .is_err()
        );
    }
    for smaa in [
        SmaaSettings {
            threshold: 0.5,
            ..valid
        },
        SmaaSettings {
            max_search_steps: 0,
            ..valid
        },
        SmaaSettings {
            max_search_steps: 112,
            ..valid
        },
        SmaaSettings {
            max_search_steps_diag: Some(0),
            ..valid
        },
        SmaaSettings {
            max_search_steps_diag: Some(20),
            ..valid
        },
        SmaaSettings {
            corner_rounding: Some(0),
            ..valid
        },
        SmaaSettings {
            corner_rounding: Some(100),
            ..valid
        },
    ] {
        assert_eq!(smaa.validate(), Ok(()), "{smaa:?}");
    }
}

#[test]
fn smaa_is_a_resolve_that_enables_the_display_path_and_excludes_fxaa() {
    let settings = HdrPostSettings {
        resolve: PostResolve::smaa(SmaaPreset::Medium),
        ..HdrPostSettings::default()
    };
    assert!(settings.enabled());
    assert_eq!(settings.validate(), Ok(()));
    // One resolve is selected, so SMAA carries no FXAA strength and vice versa.
    assert_eq!(settings.resolve.fxaa_strength(), None);
    assert_eq!(settings.resolve.active_fxaa(), None);
    assert!(PostResolve::fxaa(1.0).active_smaa().is_none());
    let bad = HdrPostSettings {
        resolve: PostResolve::Smaa(SmaaSettings {
            threshold: 0.0,
            ..SmaaSettings::LOW
        }),
        ..HdrPostSettings::default()
    };
    assert!(bad.validate().is_err());
}

#[test]
fn constants_encode_disabled_features_as_off() {
    let constant = |smaa: SmaaSettings, name: &str| {
        smaa.constants()
            .into_iter()
            .find(|(key, _)| *key == name)
            .unwrap()
            .1
    };
    assert_eq!(constant(SmaaSettings::LOW, "DIAGONAL"), 0.0);
    assert_eq!(constant(SmaaSettings::LOW, "CORNERS"), 0.0);
    assert_eq!(constant(SmaaSettings::ULTRA, "DIAGONAL"), 1.0);
    assert_eq!(constant(SmaaSettings::ULTRA, "CORNERS"), 1.0);
    assert_eq!(constant(SmaaSettings::ULTRA, "CORNER_ROUNDING"), 0.25);
    assert_eq!(constant(SmaaSettings::ULTRA, "MAX_SEARCH_STEPS_DIAG"), 16.0);
}
