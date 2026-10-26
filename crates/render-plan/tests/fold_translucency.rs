use worldrunner_render_plan::*;

fn settings() -> SceneTranslucencyParts {
    SceneTranslucencyParts {
        distortion_micro: 100_000,
        falloff_micro: 4_000_000,
        sun_scale_micro: 1_000_000,
        ambient_scale_micro: 50_000,
        scale_micro: 500_000,
        base_kelvin: 6500,
        warming_kelvin: 1000,
        luminance_micro: 300_000,
        saturation_micro: 100_000,
    }
}

#[test]
fn scattering_settings_validate_before_material_adoption() {
    let valid = SceneTranslucency::new(settings()).unwrap();
    assert_eq!(valid.parts(), settings());
    for invalid in [
        SceneTranslucencyParts {
            falloff_micro: 0,
            ..settings()
        },
        SceneTranslucencyParts {
            base_kelvin: 999,
            ..settings()
        },
        SceneTranslucencyParts {
            distortion_micro: 4_000_001,
            ..settings()
        },
        SceneTranslucencyParts {
            sun_scale_micro: 16_000_001,
            ..settings()
        },
        SceneTranslucencyParts {
            ambient_scale_micro: 16_000_001,
            ..settings()
        },
        SceneTranslucencyParts {
            scale_micro: 16_000_001,
            ..settings()
        },
        SceneTranslucencyParts {
            warming_kelvin: 40_001,
            ..settings()
        },
        SceneTranslucencyParts {
            luminance_micro: 16_000_001,
            ..settings()
        },
        SceneTranslucencyParts {
            saturation_micro: 4_000_001,
            ..settings()
        },
    ] {
        assert!(SceneTranslucency::new(invalid).is_err());
    }
}

#[test]
fn fold_height_and_minimum_weight_are_validated_and_optional() {
    let contact = SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 500).unwrap();
    assert_eq!(contact.normal_fold(), None);
    assert!(contact.with_normal_fold(0, 0).is_err());
    assert!(contact.with_normal_fold(1000, 1_000_001).is_err());
    for minimum in [0, 500_000, 1_000_000] {
        assert_eq!(
            contact
                .with_normal_fold(2000, minimum)
                .unwrap()
                .normal_fold(),
            Some([2000, minimum])
        );
    }
}
