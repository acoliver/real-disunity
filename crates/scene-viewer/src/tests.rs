use super::*;
use worldrunner_render_plan::*;

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn effects(water: bool) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: water.then_some(SceneWaterPlane {
            east_min_millimeters: 0,
            north_min_millimeters: 0,
            east_extent_millimeters: 1000,
            north_extent_millimeters: 1000,
        }),
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: SceneSun {
            direction: SceneRotation {
                x: 0,
                y: 0,
                z: ROTATION_MICRO_SCALE,
                w: 0,
            },
            // Ambient-only lighting isolates raw albedo/readback from white GGX highlights.
            irradiance_micro: 0,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
        shadows: SceneShadowCascades::new(vec![10_000], 0).expect("one unblended cascade is valid"),
        ssao: SceneSsao {
            radius_millimeters: 500,
            intensity_micro: 500_000,
            horizon_angle_millidegrees: 30_000,
        },
        camera: camera(),
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    })
    .expect("the fixture effects are valid")
}

pub(crate) fn scene(water: bool) -> RenderScene {
    let parts = SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: effects(water),
    };
    RenderScene::new(parts).unwrap()
}
pub(crate) fn options() -> ViewerOptions {
    ViewerOptions {
        title: "test".into(),
        size: [800, 600],
        pose: CameraPose {
            eye_millimeters: [0, -2000, 2000],
            target_millimeters: [0; 3],
        },
        orbit: OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 10_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
        shadows: None,
    }
}
#[test]
fn invalid_window_size_returns_before_event_loop_startup() {
    for size in [[0, 600], [800, 0], [4097, 600], [800, u32::MAX]] {
        let options = ViewerOptions { size, ..options() };
        assert!(matches!(
            run(scene(false), options),
            Err(ViewerError::InvalidOptions(_))
        ));
    }
}
#[test]
fn water_is_rejected_before_event_loop_startup() {
    assert!(matches!(
        run(scene(true), options()),
        Err(ViewerError::UnsupportedScene("water"))
    ));
}
#[test]
fn valid_scene_and_camera_need_no_display_to_validate() {
    assert!(validate(&scene(false), &options()).is_ok());
}
