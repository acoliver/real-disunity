#[test]
fn invalid_lens_and_backpressure_leave_shadow_camera_transaction_unaccepted() {
    let mut options = crate::tests::options();
    options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    let scene = crate::renderer::retained_tests::source();
    let mut host = RetainedHost::with_preparation_limit(scene, options, 1).unwrap();
    let pose = host.options().pose;
    let lens = host.options().camera;
    let convention = host.options().camera_convention;
    let mut invalid_lens = lens;
    invalid_lens.near_plane_millimeters = 0;
    let update = RetainedUpdate::camera_with_shadow_policy(
        pose,
        invalid_lens,
        convention,
        CameraContinuity::Cut,
        Some(crate::CameraShadowPolicy::Disabled),
    );

    assert!(host.apply(update).is_err());
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.readiness.shadow_policy, None);
    assert_eq!(host.options().pose, pose);
    assert!(host.readiness.queue.is_empty());

    let valid = RetainedUpdate::camera_with_shadow_policy(
        pose,
        lens,
        convention,
        CameraContinuity::Cut,
        Some(crate::CameraShadowPolicy::Disabled),
    );
    host.apply(valid).unwrap();
    let mut rejected_pose = pose;
    rejected_pose.eye_millimeters[0] += 500;
    let next = RetainedUpdate::camera_with_shadow_policy(
        rejected_pose,
        lens,
        convention,
        CameraContinuity::Continuous,
        Some(crate::CameraShadowPolicy::Enabled),
    );
    assert!(matches!(
        host.apply(next),
        Err(crate::ViewerError::BackPressure { limit: 1 })
    ));
    assert_eq!(host.accepted_generation(), 1);
    assert_eq!(host.options().pose, pose);
    assert_eq!(host.readiness.queue.len(), 1);
    assert_eq!(host.readiness.shadow_policy, None);
}

#[test]
fn shadow_policy_publishes_with_camera_and_survives_legacy_updates() {
    let mut options = crate::tests::options();
    options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    let mut host = RetainedHost::new(crate::renderer::retained_tests::source(), options).unwrap();
    let options = host.options();
    let mut pose = options.pose;
    pose.eye_millimeters[0] += 500;
    let lens = options.camera;
    let convention = options.camera_convention;
    let before = host.view().unwrap().unwrap().view_matrix();
    let policy = crate::CameraShadowPolicy::Disabled;

    host.apply(RetainedUpdate::camera_with_shadow_policy(
        pose,
        lens,
        convention,
        CameraContinuity::Continuous,
        Some(policy),
    ))
    .unwrap();
    assert_eq!(host.readiness.shadow_policy, None);
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before);
    host.readiness.publish_front();
    assert_eq!(host.readiness.shadow_policy, Some(policy));
    assert_ne!(host.view().unwrap().unwrap().view_matrix(), before);

    host.apply(RetainedUpdate::Camera {
        pose,
        lens,
        convention,
    })
    .unwrap();
    assert_eq!(host.readiness.shadow_policy, Some(policy));
    host.readiness.publish_front();
    assert_eq!(host.readiness.shadow_policy, Some(policy));
}

#[test]
fn plain_scene_rejects_shadow_enable_without_accepting_camera_transaction() {
    let scene = crate::tests::scene(false);
    let mut host = RetainedHost::new(scene, crate::tests::options()).unwrap();
    let before = host.view().unwrap().unwrap().view_matrix();
    let options = host.options();
    let accepted_pose = options.pose;
    let mut pose = options.pose;
    pose.eye_millimeters[0] += 500;
    let update = RetainedUpdate::camera_with_shadow_policy(
        pose,
        options.camera,
        options.camera_convention,
        CameraContinuity::Cut,
        Some(crate::CameraShadowPolicy::Enabled),
    );

    assert!(matches!(
        host.apply(update),
        Err(crate::ViewerError::UnsupportedScene(_))
    ));
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.readiness.published, 0);
    assert_eq!(host.options().pose, accepted_pose);
    assert!(host.readiness.queue.is_empty());
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before);
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::None
    );
}

#[test]
fn plain_scene_accepts_shadow_disable_without_shadow_resources() {
    let scene = crate::tests::scene(false);
    let mut host = RetainedHost::new(scene, crate::tests::options()).unwrap();
    let options = host.options();
    let update = RetainedUpdate::camera_with_shadow_policy(
        options.pose,
        options.camera,
        options.camera_convention,
        CameraContinuity::Continuous,
        Some(crate::CameraShadowPolicy::Disabled),
    );
    host.apply(update).unwrap();

    assert_eq!(host.readiness.shadow_policy, None);
    host.readiness.publish_front();
    assert_eq!(
        host.readiness.shadow_policy,
        Some(crate::CameraShadowPolicy::Disabled)
    );
}

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
fn no_camera_update_keeps_published_shadow_policy_and_camera_generation() {
    let mut options = crate::tests::options();
    options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    let mut host = RetainedHost::new(crate::renderer::retained_tests::source(), options).unwrap();
    let before = host.view().unwrap().unwrap().view_matrix();
    let policy = crate::CameraShadowPolicy::Disabled;
    host.apply(RetainedUpdate::camera_with_shadow_policy(
        host.options().pose,
        host.options().camera,
        host.options().camera_convention,
        CameraContinuity::Continuous,
        Some(policy),
    ))
    .unwrap();
    host.readiness.publish_front();
    let generation = host.readiness.published;

    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before);
    assert_eq!(host.readiness.shadow_policy, Some(policy));
    assert_eq!(host.readiness.published, generation);
    assert!(host.readiness.queue.is_empty());
}

fn shadow_camera_host() -> RetainedHost {
    let mut options = crate::tests::options();
    options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    RetainedHost::new(crate::renderer::retained_tests::source(), options).unwrap()
}

#[test]
fn queued_shadow_requests_preserve_omission_and_cut_until_submission() {
    let mut host = shadow_camera_host();
    let (initial_pose, lens, convention) = host.readiness.camera;
    let disabled = crate::CameraShadowPolicy::Disabled;
    let enabled = crate::CameraShadowPolicy::Enabled;
    let requests = [
        (Some(disabled), CameraContinuity::Cut),
        (None, CameraContinuity::Continuous),
        (Some(enabled), CameraContinuity::Continuous),
    ];
    let mut poses = Vec::new();
    for (index, (policy, continuity)) in requests.into_iter().enumerate() {
        let mut pose = initial_pose;
        pose.eye_millimeters[0] += (i64::try_from(index).unwrap() + 1) * 500;
        poses.push(pose);
        host.apply(RetainedUpdate::camera_with_shadow_policy(
            pose, lens, convention, continuity, policy,
        ))
        .unwrap();
    }
    host.apply(RetainedUpdate::camera_with_continuity(
        initial_pose,
        lens,
        convention,
        CameraContinuity::Continuous,
    ))
    .unwrap();
    poses.push(initial_pose);
    assert_eq!(host.accepted_generation(), 4);
    assert_eq!(host.readiness.published, 0);
    assert_eq!(host.readiness.camera.0, initial_pose);
    assert_eq!(host.readiness.shadow_policy, None);
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::None
    );
    for (index, policy) in [disabled, disabled, enabled, enabled]
        .into_iter()
        .enumerate()
    {
        host.readiness.publish_front();
        assert_eq!(host.readiness.published, index as u64 + 1);
        assert_eq!(host.readiness.camera.0, poses[index]);
        assert_eq!(host.readiness.shadow_policy, Some(policy));
        assert_eq!(
            host.readiness.camera_history.reset_reason(),
            TemporalHistoryReset::CameraCut
        );
    }
    assert!(host.readiness.queue.is_empty());
    host.readiness.camera_history.submitted();
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::None
    );
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
