use super::*;

fn input(frame: u64) -> CascadeRefreshInput {
    CascadeRefreshInput {
        frame,
        matrix: Mat4::IDENTITY,
        toward_light: Vec3::Z,
        resolution: 1024,
        generation: 0,
        capture: false,
    }
}

fn staggered() -> CascadeRefreshPolicy {
    CascadeRefreshPolicy {
        intervals: [1, 2, 4, 4],
        phases: [0, 0, 1, 3],
        max_non_near: 1,
        light_delta_radians: 0.01,
        projection_delta_texels: 2.0,
    }
}

#[test]
fn default_refreshes_every_cascade_every_frame() {
    let policy = CascadeRefreshPolicy::default();
    let mut states = [CascadeRefreshState::default(); 4];
    for frame in 0..12 {
        for (index, state) in states.iter_mut().enumerate() {
            let request = input(frame);
            let reason = policy.reason(index, state, request).unwrap();
            state.record(request, reason);
            assert_eq!(state.last_frame(), Some(frame));
            assert_eq!(state.matrix(), Some(request.matrix));
        }
    }
}

#[test]
fn staggered_refreshes_obey_budget_and_intervals() {
    let policy = staggered();
    assert!(policy.validate().is_ok());
    let mut states = [CascadeRefreshState::default(); 4];
    for state in &mut states {
        state.record(input(0), RefreshReason::Uninitialized);
    }
    for frame in 1..33 {
        let mut far_count = 0;
        for (index, state) in states.iter_mut().enumerate() {
            if let Some(reason) = policy.reason(index, state, input(frame)) {
                far_count += usize::from(index > 0);
                state.record(input(frame), reason);
            }
            assert!(frame - state.last_frame().unwrap() < u64::from(policy.intervals[index]));
        }
        assert!(far_count <= 1);
        assert_eq!(states[0].last_frame(), Some(frame));
    }
}

#[test]
fn forced_triggers_refresh_immediately_and_keep_rendered_matrix_until_recorded() {
    let policy = staggered();
    let mut state = CascadeRefreshState::default();
    state.record(input(0), RefreshReason::Uninitialized);
    assert_eq!(policy.reason(2, &state, input(2)), None);
    let mut cases = Vec::new();
    let mut changed = input(2);
    changed.toward_light = Vec3::new(0.1, 0.0, 1.0).normalize();
    cases.push((changed, RefreshReason::Light));
    changed = input(2);
    changed.matrix.w_axis.x += 5.0 / 512.0;
    cases.push((changed, RefreshReason::Projection));
    changed = input(2);
    changed.matrix.z_axis.z = 0.5;
    cases.push((changed, RefreshReason::DepthRange));
    changed = input(2);
    changed.generation = 1;
    cases.push((changed, RefreshReason::Generation));
    changed = input(2);
    changed.resolution = 512;
    cases.push((changed, RefreshReason::Resolution));
    changed = input(2);
    changed.capture = true;
    cases.push((changed, RefreshReason::Capture));
    for (request, expected) in cases {
        assert_eq!(policy.reason(2, &state, request), Some(expected));
        assert_eq!(state.matrix(), Some(Mat4::IDENTITY));
    }
    state.invalidate();
    assert_eq!(
        policy.reason(2, &state, input(2)),
        Some(RefreshReason::Invalidated)
    );
}

#[test]
fn motion_is_measured_against_rendered_map_in_texels() {
    let policy = staggered();
    let mut state = CascadeRefreshState::default();
    state.record(input(0), RefreshReason::Uninitialized);
    let mut moved = input(2);
    moved.matrix.w_axis.x = 1.0 / 512.0;
    assert_eq!(policy.reason(2, &state, moved), None);
    moved.matrix.w_axis.x = 3.0 / 512.0;
    assert_eq!(
        policy.reason(2, &state, moved),
        Some(RefreshReason::Projection)
    );
    state.record(moved, RefreshReason::Projection);
    assert_eq!(state.matrix(), Some(moved.matrix));
}

#[test]
fn invalid_policies_reject_overlapping_periodic_slots() {
    let mut policy = staggered();
    policy.phases[3] = 1;
    assert!(policy.validate().is_err());
    policy = staggered();
    policy.intervals[1] = 0;
    assert!(policy.validate().is_err());
    policy = staggered();
    policy.projection_delta_texels = f32::NAN;
    assert!(policy.validate().is_err());
}
