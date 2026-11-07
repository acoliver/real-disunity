use super::*;
use std::cell::Cell;

fn host() -> RetainedHost {
    RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
    )
    .unwrap()
}

fn update(continuity: CameraContinuity) -> RetainedUpdate {
    let options = crate::tests::options();
    RetainedUpdate::camera_with_continuity(
        options.pose,
        options.camera,
        options.camera_convention,
        continuity,
    )
}

#[test]
fn cut_and_camera_become_visible_in_the_same_published_generation() {
    let mut host = host();
    let before = host.view().unwrap().unwrap().view_matrix();
    let options = host.options();
    let mut pose = options.pose;
    pose.eye_millimeters[0] += 500;
    let update = RetainedUpdate::camera_with_continuity(
        pose,
        options.camera,
        options.camera_convention,
        CameraContinuity::Cut,
    );
    host.apply(update).unwrap();
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before);
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::None
    );
    host.readiness.publish_front();
    assert_ne!(host.view().unwrap().unwrap().view_matrix(), before);
    assert_eq!(host.ready_generation(), 1);
    let reset = Cell::new(false);
    assert_eq!(
        host.readiness.camera_history.prepare(|| reset.set(true)),
        TemporalHistoryReset::CameraCut
    );
    assert!(reset.get());
}

#[test]
fn existing_camera_updates_default_to_continuous() {
    let mut host = host();
    let options = host.options();
    let update = RetainedUpdate::Camera {
        pose: options.pose,
        lens: options.camera,
        convention: options.camera_convention,
    };
    assert_eq!(CameraContinuity::default(), CameraContinuity::Continuous);
    host.apply(update).unwrap();
    host.readiness.publish_front();
    let calls = Cell::new(0);
    assert_eq!(
        host.readiness
            .camera_history
            .prepare(|| calls.set(calls.get() + 1)),
        TemporalHistoryReset::None
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn cuts_wait_for_publication_and_collapse_before_submission() {
    let mut host = host();
    host.apply(update(CameraContinuity::Cut)).unwrap();
    host.apply(update(CameraContinuity::Cut)).unwrap();
    host.apply(update(CameraContinuity::Continuous)).unwrap();
    let calls = Cell::new(0);
    let history = &host.readiness.camera_history;
    assert_eq!(history.prepare(|| calls.set(1)), TemporalHistoryReset::None);
    assert_eq!(calls.get(), 0);
    while !host.readiness.queue.is_empty() {
        host.readiness.publish_front();
    }
    assert_eq!(
        host.readiness
            .camera_history
            .prepare(|| calls.set(calls.get() + 1)),
        TemporalHistoryReset::CameraCut
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(host.ready_generation(), 3);
    host.readiness.camera_history.submitted();
    assert_eq!(
        host.readiness
            .camera_history
            .prepare(|| calls.set(calls.get() + 1)),
        TemporalHistoryReset::None
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn abandoned_preparation_keeps_cut_until_submission() {
    let mut host = host();
    host.apply(update(CameraContinuity::Cut)).unwrap();
    host.readiness.publish_front();
    let calls = Cell::new(0);
    for _ in 0..2 {
        assert_eq!(
            host.readiness
                .camera_history
                .prepare(|| calls.set(calls.get() + 1)),
            TemporalHistoryReset::CameraCut
        );
    }
    assert_eq!(calls.get(), 2);
    host.readiness.camera_history.submitted();
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::None
    );
}

#[test]
fn invalid_cut_leaves_camera_and_pending_history_unchanged() {
    let mut host = host();
    let options = crate::tests::options();
    let mut lens = options.camera;
    lens.near_plane_millimeters = 0;
    assert!(
        host.apply(RetainedUpdate::camera_with_continuity(
            options.pose,
            lens,
            options.camera_convention,
            CameraContinuity::Cut,
        ))
        .is_err()
    );
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::None
    );
}
