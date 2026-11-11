//! The viewer Play starts from the first frame's lights, drawn on the GPU.
use super::*;
use worldrunner_disunity_runtime::PresentationLight;
use worldrunner_scene_viewer::{RetainedDriver, RetainedHost, RetainedTick};
use worldrunner_world_simulation::presentation::{LightKind, LightSettings, LightShadowMode};

use crate::scene_light::SceneLighting;

/// A driver that changes nothing between frames.
struct Still;

impl RetainedDriver for Still {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        Ok(())
    }
}

/// A directional light pointing straight down.
fn sun(shadows: LightShadowMode) -> PresentationLight {
    PresentationLight {
        object: 0,
        local_to_world: [
            1., 0., 0., 0., 0., -1., 0., 0., 0., 0., -1., 0., 0., 0., 10., 1.,
        ],
        settings: LightSettings {
            kind: LightKind::Directional,
            shadows,
            ..LightSettings::default()
        },
    }
}

/// A viewer started as `play_player_window` starts one, showing a triangle one metre above
/// another.
fn started(lights: &[PresentationLight]) -> RetainedHost {
    let mut options = viewer_options(&PlayerWindow {
        title: "lit tables".into(),
        size: [64, 64],
        fixed_dt: None,
        max_frames: None,
        capture_frames: BTreeSet::new(),
        keys: Vec::new(),
    });
    let (lighting, shadows) = SceneLighting::start(lights, options.camera).unwrap();
    options.shadows = shadows;
    let scene = SceneTables::initial_scene(lighting.effects().cloned()).unwrap();
    let mut viewer = RetainedHost::new(scene, options).unwrap();
    let mut tables = SceneTables::new().unwrap();
    let objects = [
        object(1, 100, at(0., 0., 0.)),
        object(2, 100, at(0., 0., 1.)),
    ];
    for edit in update(&mut tables, &objects, &content(&[100])).into_updates() {
        viewer.apply(edit).unwrap();
    }
    viewer
}

/// Whether the first frame of `viewer` drew into any shadow cascade.
fn draws_casters(viewer: RetainedHost) -> bool {
    let mut driver = Still;
    let mut run = viewer.windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    let drew = run
        .host()
        .statistics()
        .unwrap()
        .scene
        .passes
        .shadow_cascades
        .iter()
        .any(|cascade| cascade.calls > 0);
    run.shutdown().unwrap();
    drew
}

#[test]
fn a_shadow_casting_sun_starts_a_viewer_that_draws_shadow_casters() {
    assert!(draws_casters(started(&[sun(LightShadowMode::Soft)])));
}

#[test]
fn a_sun_without_shadows_or_no_sun_draws_no_shadow_casters() {
    assert!(!draws_casters(started(&[sun(LightShadowMode::None)])));
    assert!(!draws_casters(started(&[])));
}
