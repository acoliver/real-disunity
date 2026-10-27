use super::*;
use crate::scene::direction::SceneDirection;

fn parts() -> SceneEffectsParts {
    SceneEffectsParts {
        water: Some(water_body()),
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 96,
                green: 148,
                blue: 220,
            },
            horizon: SceneColor {
                red: 208,
                green: 226,
                blue: 240,
            },
        },
        sun: SceneSun {
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 800_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 200_000,
            ground_irradiance_micro: 80_000,
        },
        shadows: SceneShadowCascades::new(vec![8_000, 32_000], 10_000)
            .expect("synthetic cascades validate"),
        ssao: Some(SceneSsao {
            kernel: crate::SceneSsaoKernel::Hemisphere,
            filter: crate::SceneSsaoFilter::DepthAwareBox,
            sample_count: 16,
            bias_millimeters: 10,
            radius_millimeters: 1_200,
            intensity_micro: 1_000_000,
            horizon_angle_millidegrees: 90_000,
        }),
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 2_000_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![ScenePostStage::Bloom {
            threshold_micro: 900_000,
            intensity_micro: 250_000,
        }],
        anti_alias: SceneAntiAlias::Fxaa {
            subpixel_micro: 250_000,
        },
    }
}

#[test]
fn nonneutral_grading_cannot_construct_scene_effects() {
    for (contrast, saturation, part, observed) in [
        (999_999, 1_000_000, "grading contrast_micro", 999_999),
        (1_000_000, 1_000_001, "grading saturation_micro", 1_000_001),
        (0, 2_000_000, "grading contrast_micro", 0),
    ] {
        let mut staged = parts();
        staged.grading.contrast_micro = contrast;
        staged.grading.saturation_micro = saturation;
        assert_eq!(
            SceneEffects::new(staged),
            Err(SceneValidationError::UnsupportedColorGrading {
                part,
                observed,
                neutral: EFFECTS_MICRO_SCALE,
            })
        );
    }
}

#[test]
fn grading_quantity_bounds_still_fail_before_capability_checks() {
    for (contrast, saturation, part) in [
        (2_000_001, 1_000_000, "grading contrast_micro"),
        (1_000_000, u32::MAX, "grading saturation_micro"),
    ] {
        let mut staged = parts();
        staged.grading.contrast_micro = contrast;
        staged.grading.saturation_micro = saturation;
        assert!(matches!(
            SceneEffects::new(staged),
            Err(SceneValidationError::RangeViolation { part: actual, .. }) if actual == part
        ));
    }
}

#[test]
fn neutral_grading_preserves_the_exposure_domain() {
    for exposure in [0, 1_000_000, 1_750_000, 4_000_000] {
        let mut staged = parts();
        staged.grading.exposure_micro = exposure;
        let expected = staged.grading;
        let effects = SceneEffects::new(staged).expect("neutral grading is supported");
        assert_eq!(*effects.grading(), expected);
    }
    let mut staged = parts();
    staged.grading.exposure_micro = 4_000_001;
    assert!(matches!(
        SceneEffects::new(staged),
        Err(SceneValidationError::RangeViolation {
            part: "grading exposure_micro",
            ..
        })
    ));
}

#[test]
fn valid_effects_compose_with_water() {
    let effects = SceneEffects::new(parts()).expect("synthetic effects validate");
    assert!(effects.water().is_some());
    assert_eq!(effects.shadows().extents_millimeters(), &[8_000, 32_000]);
    assert_eq!(effects.post().len(), 1);
}

#[test]
fn cascade_extents_must_strictly_increase() {
    assert!(matches!(
        SceneShadowCascades::new(vec![8_000, 8_000], 0),
        Err(SceneValidationError::CascadeNotIncreasing { position: 1, .. })
    ));
    assert!(matches!(
        SceneShadowCascades::new(Vec::new(), 0),
        Err(SceneValidationError::EmptyCollection { .. })
    ));
}

#[test]
fn camera_planes_must_be_ordered() {
    let mut staged = parts();
    staged.camera.far_plane_millimeters = staged.camera.near_plane_millimeters;
    assert!(matches!(
        SceneEffects::new(staged),
        Err(SceneValidationError::PlaneOrdering { .. })
    ));
}

#[test]
fn quantity_bounds_fail_closed() {
    // The sun carries its own, higher ceiling; see
    // MAXIMUM_SUN_IRRADIANCE_MICRO for why it is not a unit ratio.
    let mut staged = parts();
    staged.sun.irradiance_micro = MAXIMUM_SUN_IRRADIANCE_MICRO + 1;
    assert!(matches!(
        SceneEffects::new(staged),
        Err(SceneValidationError::RangeViolation {
            observed: 10_000_001,
            ..
        })
    ));
    let mut staged = parts();
    staged.anti_alias = SceneAntiAlias::Taa {
        history_micro: 1_000_001,
    };
    assert!(matches!(
        SceneEffects::new(staged),
        Err(SceneValidationError::RangeViolation { .. })
    ));
    let mut staged = parts();
    staged.water = Some(SceneWaterBody {
        material: crate::SceneWaterSettings::STOCK,
        surface: crate::SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: 0,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
        east_min_millimeters: i64::MAX,
        north_min_millimeters: 0,
        east_extent_millimeters: 2_000,
        north_extent_millimeters: 2_000,
    });
    assert!(matches!(
        SceneEffects::new(staged),
        Err(SceneValidationError::ArithmeticOverflow { .. })
    ));
}

#[test]
fn water_and_ssao_can_be_omitted_within_a_profile() {
    let mut staged = parts();
    staged.water = None;
    staged.ssao = None;
    let effects = SceneEffects::new(staged).expect("waterless effects validate");
    assert!(effects.water().is_none());
    assert!(effects.ssao().is_none());
}

fn water_body() -> SceneWaterBody {
    SceneWaterBody {
        material: crate::SceneWaterSettings::STOCK,
        surface: crate::SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: 0,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
        east_min_millimeters: -1_000,
        north_min_millimeters: -1_000,
        east_extent_millimeters: 2_000,
        north_extent_millimeters: 2_000,
    }
}

#[test]
fn water_body_collection_replaces_single_input_and_validates_every_body() {
    let mut elevated = water_body();
    elevated.height_millimeters = 4000;
    let effects = SceneEffects::new(parts())
        .unwrap()
        .with_water_bodies(vec![water_body(), elevated.clone()])
        .unwrap();
    assert_eq!(effects.water_bodies().len(), 2);
    assert_eq!(effects.water_bodies()[1], elevated);
    let empty = effects.clone().with_water_bodies(Vec::new()).unwrap();
    assert!(empty.water().is_none());
    elevated.surface = crate::SceneWaterSurface::Basin {
        columns: 2,
        rows: 2,
        bed_millimeters: vec![0; 3].into_boxed_slice(),
    };
    assert!(
        effects
            .with_water_bodies(vec![water_body(), elevated])
            .is_err()
    );
}

#[test]
fn scene_specular_selection_is_explicit_and_default_is_unchanged() {
    use crate::{
        SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularOcclusion,
        SceneSpecularVisibility,
    };
    let effects = SceneEffects::new(parts()).unwrap();
    assert_eq!(
        effects.specular_convention(),
        SceneSpecularConvention::default()
    );
    assert_eq!(effects.specular_convention().shader_options(), [0; 6]);
    let selected = SceneSpecularConvention {
        roughness: SceneRoughnessRemap::Perceptual04,
        visibility: SceneSpecularVisibility::SchlickSmith,
        occlusion: SceneSpecularOcclusion::RoughnessAware,
        ..Default::default()
    };
    let effects = effects.with_specular_convention(selected);
    assert_eq!(effects.specular_convention(), selected);
    assert_eq!(selected.shader_options(), [1, 1, 1, 0, 0, 0]);
}
