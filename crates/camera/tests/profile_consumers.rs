//! Every leaf field of the camera profile reaches the camera that consumes it: changing the field
//! in an otherwise identical profile changes what the camera does, and by the amount the field
//! states.

use std::time::Duration;

use glam::DVec3;
use worldrunner_camera::{CameraProfile, FreeFlyCamera, RtsCamera, RtsCameraInput, ViewHandedness};

fn profile() -> CameraProfile {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/camera-profile-rts-v1.json");
    CameraProfile::load(&path).expect("authored camera profile loads")
}

fn rts(profile: &CameraProfile) -> RtsCamera {
    RtsCamera::new(profile).expect("profile validates")
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-9,
        "{actual} is not {expected}"
    );
}

/// Distance the ground target moves in 100 ms of forward panning at the default mid zoom.
fn pan_distance(profile: &CameraProfile, speed_multiplier: f64) -> f64 {
    let mut camera = rts(profile);
    camera.update(
        RtsCameraInput {
            forward: 1.0,
            speed_multiplier,
            ..RtsCameraInput::default()
        },
        Duration::from_millis(100),
    );
    camera.ground_target.length()
}

/// Mid-zoom pan speed blends the close-zoom speed with the zoomed-out speed.
fn mid_zoom_pan_speed(close: f64, zoomed: f64) -> f64 {
    close.midpoint(zoomed)
}

#[test]
fn reaches_consumer_handedness() {
    let mut profile = profile();
    profile.view_convention.handedness = ViewHandedness::RightHanded;
    assert_eq!(rts(&profile).handedness, ViewHandedness::RightHanded);
    profile.view_convention.handedness = ViewHandedness::DirectXLeftHanded;
    assert_eq!(rts(&profile).handedness, ViewHandedness::DirectXLeftHanded);
    assert_eq!(
        FreeFlyCamera::new(&profile, DVec3::ZERO, 0.0, 0.0, 1.0).handedness,
        ViewHandedness::DirectXLeftHanded
    );
}

#[test]
fn reaches_consumer_default_speed_meters_per_second() {
    let mut profile = profile();
    profile.free_fly.default_speed_meters_per_second = 123.0;
    near(
        FreeFlyCamera::new(&profile, DVec3::ZERO, 0.0, 0.0, 1.0).movement_speed_meters_per_second,
        123.0,
    );
}

#[test]
fn reaches_consumer_move_speed_normal() {
    let mut profile = profile();
    profile.rts.move_speed_normal = 16.0;
    near(
        pan_distance(&profile, 1.0),
        mid_zoom_pan_speed(16.0, profile.rts.zoomed_pan_speed) * 0.1,
    );
}

#[test]
fn reaches_consumer_move_speed_fast() {
    let mut profile = profile();
    profile.rts.move_speed_fast = 40.0;
    near(
        pan_distance(&profile, 2.5),
        mid_zoom_pan_speed(40.0, profile.rts.zoomed_pan_speed) * 2.5 * 0.1,
    );
}

#[test]
fn reaches_consumer_zoomed_pan_speed() {
    let mut profile = profile();
    profile.rts.zoomed_pan_speed = 500.0;
    near(
        pan_distance(&profile, 1.0),
        mid_zoom_pan_speed(profile.rts.move_speed_normal, 500.0) * 0.1,
    );
}

#[test]
fn reaches_consumer_zoom_min_m() {
    let mut profile = profile();
    profile.rts.zoom_min_m = 10.0;
    near(
        rts(&profile).distance_meters(),
        10.0_f64.midpoint(profile.rts.zoom_max_m),
    );
}

#[test]
fn reaches_consumer_zoom_max_m() {
    let mut profile = profile();
    profile.rts.zoom_max_m = 100.0;
    near(
        rts(&profile).distance_meters(),
        profile.rts.zoom_min_m.midpoint(100.0),
    );
}

#[test]
fn reaches_consumer_pitch_min_deg() {
    let mut profile = profile();
    profile.rts.pitch_min_deg = -45.0;
    let mut camera = rts(&profile);
    camera.update(
        RtsCameraInput {
            pitch_delta_radians: -10.0,
            ..RtsCameraInput::default()
        },
        Duration::from_millis(16),
    );
    near(camera.pitch_radians, (-45.0_f64).to_radians());
}

#[test]
fn reaches_consumer_pitch_max_deg() {
    let mut profile = profile();
    profile.rts.pitch_max_deg = -20.0;
    let mut camera = rts(&profile);
    camera.update(
        RtsCameraInput {
            pitch_delta_radians: 10.0,
            ..RtsCameraInput::default()
        },
        Duration::from_millis(16),
    );
    near(camera.pitch_radians, (-20.0_f64).to_radians());
}

#[test]
fn reaches_consumer_rotation_speed_radians_per_second() {
    let mut profile = profile();
    profile.rts.rotation_speed_radians_per_second = 1.5;
    let mut camera = rts(&profile);
    camera.update(
        RtsCameraInput {
            rotate: 1.0,
            ..RtsCameraInput::default()
        },
        Duration::from_millis(100),
    );
    near(camera.yaw_radians.abs(), 0.15);
}

#[test]
fn reaches_consumer_zoom_step() {
    let mut profile = profile();
    profile.rts.zoom_step = 0.3;
    let mut camera = rts(&profile);
    let before = camera.target_zoom;
    camera.zoom_by_steps(1.0);
    near(camera.target_zoom, before - 0.3);
}

#[test]
fn reaches_consumer_zoom_time_seconds() {
    let mut slow = profile();
    slow.rts.zoom_time_seconds = 1.0;
    let mut camera = rts(&slow);
    camera.zoom_by_steps(1.0);
    camera.update(RtsCameraInput::default(), Duration::from_millis(100));
    // A tenth of the way through a 1 s transition: sine-out progress of 0.1.
    let progress = 0.1_f64 * std::f64::consts::FRAC_PI_2;
    let expected = 0.5 + (camera.target_zoom - 0.5) * progress.sin();
    near(camera.zoom, expected);
}

#[test]
fn reaches_consumer_look_at_height_min_m() {
    let mut profile = profile();
    profile.rts.look_at_height_min_m = 1.0;
    near(
        rts(&profile).look_at_height_meters(),
        1.0_f64.midpoint(profile.rts.look_at_height_max_m),
    );
}

#[test]
fn reaches_consumer_look_at_height_max_m() {
    let mut profile = profile();
    profile.rts.look_at_height_max_m = 30.0;
    near(
        rts(&profile).look_at_height_meters(),
        profile.rts.look_at_height_min_m.midpoint(30.0),
    );
}

#[test]
fn reaches_consumer_hfov_by_aspect() {
    let mut profile = profile();
    profile.rts.hfov_by_aspect = vec![(1.0, 60.0), (2.0, 100.0)];
    let camera = rts(&profile);
    near(
        camera.horizontal_fov_radians_for_aspect(1.0),
        60.0_f64.to_radians(),
    );
    near(
        camera.horizontal_fov_radians_for_aspect(2.0),
        100.0_f64.to_radians(),
    );
}
