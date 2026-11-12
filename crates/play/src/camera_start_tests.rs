use glam::DVec3;
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ViewConvention, ViewHandedness, Viewport,
};

use super::*;

fn state(eye_x: f64, lens: CameraLens) -> CameraState {
    CameraState {
        pose: CameraPose {
            eye_meters: DVec3::new(eye_x, -10.0, 5.0),
            forward: DVec3::new(0.0, 1.0, 0.0),
            up: DVec3::Z,
        },
        lens,
        viewport: Viewport {
            width: 960,
            height: 540,
        },
        convention: ViewConvention {
            handedness: ViewHandedness::DirectXLeftHanded,
        },
    }
}

fn perspective(eye_x: f64) -> CameraState {
    state(
        eye_x,
        CameraLens::Perspective {
            vertical_fov_radians: 1.0,
            near_meters: 0.3,
            far_meters: 1000.0,
        },
    )
}

fn orthographic() -> CameraState {
    state(
        0.0,
        CameraLens::Orthographic {
            half_height_meters: 5.0,
            near_meters: 0.3,
            far_meters: 1000.0,
        },
    )
}

#[test]
fn a_camera_the_game_presents_a_frame_late_still_starts_the_window() {
    let mut start = CameraStart::default();
    assert_eq!(
        start.next(false, Some(&perspective(1.0))).start,
        None,
        "no objects yet"
    );
    let missing = start.next(true, None);
    assert_eq!(missing.start, None);
    assert!(missing.note.is_some_and(|note| note.contains("no camera")));
    let camera = perspective(2.0);
    assert_eq!(
        start.next(true, Some(&camera)).start,
        Some(scene_camera(&camera).unwrap()),
        "the next frame with objects starts at the camera that has appeared"
    );
}

#[test]
fn a_refused_camera_is_reported_and_a_usable_one_starts_the_window() {
    let mut start = CameraStart::default();
    let refused = start.next(true, Some(&orthographic()));
    assert_eq!(refused.start, None);
    assert!(refused.note.is_some_and(|note| note.contains("not shown")));
    assert!(start.next(true, Some(&perspective(1.0))).start.is_some());
}

#[test]
fn once_started_a_changed_game_camera_leaves_the_view_to_navigation() {
    let mut start = CameraStart::default();
    assert!(start.next(true, Some(&perspective(1.0))).start.is_some());
    let later = start.next(true, Some(&perspective(9.0)));
    assert_eq!(
        later,
        StartStep {
            start: None,
            note: None
        }
    );
}

#[test]
fn each_reason_the_window_keeps_its_view_is_reported_once() {
    let mut start = CameraStart::default();
    let notes: Vec<_> = [None, None, Some(orthographic()), Some(orthographic()), None]
        .iter()
        .filter_map(|game| start.next(true, game.as_ref()).note)
        .collect();
    assert_eq!(notes.len(), 3, "missing, refused, missing again: {notes:?}");
    assert!(notes[0].contains("no camera") && notes[2].contains("no camera"));
    assert!(notes[1].contains("not shown"));
}
