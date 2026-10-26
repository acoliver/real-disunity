use super::*;
use worldrunner_render_plan::SceneDirection;
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
        water: water.then_some(SceneWaterBody {
            material: worldrunner_render_plan::SceneWaterSettings::STOCK,
            surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
            flow_millimeters_per_second: [0; 2],
            height_millimeters: 0,
            color_micro: [50_000, 200_000, 250_000],
            absorption_micro: [300_000, 100_000, 50_000],
            roughness_micro: 200_000,
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
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            // Ambient-only lighting isolates raw albedo/readback from white GGX highlights.
            irradiance_micro: 0,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
        shadows: SceneShadowCascades::new(vec![1_000, 2_000, 4_000, 8_000], 100_000)
            .expect("four cascades are valid"),
        ssao: None,
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
        effects: Some(effects(water)),
    };
    RenderScene::new(parts).unwrap()
}
pub(crate) fn options() -> ViewerOptions {
    ViewerOptions {
        frame_profile: crate::FrameProfile::Legacy,
        frames_in_flight: 2,
        cadence: crate::PresentationCadence::default(),
        render_clock: crate::RenderClock::default(),
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
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
        Err(ViewerError::UnsupportedScene("water requires HDR"))
    ));
}
#[test]
fn valid_scene_and_camera_need_no_display_to_validate() {
    assert!(validate(&scene(false), &options()).is_ok());
}
