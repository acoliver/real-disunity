#[test]
fn authored_range_policy_reaches_delayed_and_same_frame_fitting() {
    use worldrunner_render_plan::{
        SceneDepthAdoption, SceneDepthFitting, SceneDepthMinimumSpan, SceneDepthSplits,
    };
    let mut authored = SceneDepthFitting {
        near_step_mm: 200,
        far_step_mm: 10_000,
        minimum_span_mm: 200_000,
        minimum_span_policy: SceneDepthMinimumSpan::ExtendFar,
        active_maximum_mm: Some(1_100_000),
        splits: SceneDepthSplits::Practical {
            lambda_micro: 500_000,
        },
        adoption: SceneDepthAdoption::FrameIndexed {
            latency_frames: 1,
            contraction_frames: 0,
        },
    };
    assert!(authored.validate().is_ok());
    let settings = SdsmSettings::from(&authored);
    assert_eq!(settings.range.active_maximum, Some(1100.0));
    let depth = DepthRange::Receivers {
        near: 1000.0,
        far: 1050.0,
    };
    for (policy, frame) in [(settings.policy, 2), (AdoptionPolicy::SameFrameCapture, 1)] {
        let mut state = AdoptionState::new(SdsmSettings { policy, ..settings }).unwrap();
        state.complete(&state.ticket(1), depth).unwrap();
        assert_eq!(
            state.adopt(frame, [0.1, 2000.0]).unwrap().unwrap().depth,
            DepthRange::Receivers {
                near: 1000.0,
                far: 1100.0
            }
        );
    }
    authored.active_maximum_mm = Some(0);
    assert!(authored.validate().is_err());
}

#[test]
fn pending_invalidation_preserves_each_contraction_timer_and_rejects_old_evidence() {
    use std::time::Duration;
    let mut state = AdoptionState::new(config(AdoptionPolicy::QuantizedPerBound {
        latency_frames: 1,
        contraction_delay: Duration::from_secs(100),
    }))
    .unwrap();
    let r = |near, far| DepthRange::Receivers { near, far };
    for (frame, seconds, depth) in [
        (1, 0, r(2.0, 20.0)),
        (2, 10, r(4.0, 20.0)),
        (3, 20, r(4.0, 18.0)),
    ] {
        state.complete(&state.ticket(frame - 1), depth).unwrap();
        assert_eq!(
            state
                .adopt_at(frame, [0.1, 40.0], Duration::from_secs(seconds))
                .unwrap()
                .unwrap()
                .depth,
            r(2.0, 20.0)
        );
    }
    let old_ticket = state.ticket(3);
    state.complete(&old_ticket, r(1.0, 30.0)).unwrap();
    state.invalidate_pending();
    assert!(matches!(
        state.complete(&old_ticket, r(1.0, 30.0)),
        Err(SdsmError::StaleResource)
    ));
    assert!(
        state
            .adopt_at(4, [0.1, 40.0], Duration::from_secs(30))
            .unwrap()
            .is_none()
    );
    for (frame, seconds, candidate, expected) in [
        (5, 30, r(4.0, 18.0), r(2.0, 20.0)),
        (6, 110, r(4.0, 18.0), r(4.0, 20.0)),
        (7, 120, r(4.0, 18.0), r(4.0, 18.0)),
        (8, 121, r(1.0, 30.0), r(1.0, 30.0)),
    ] {
        state.complete(&state.ticket(frame - 1), candidate).unwrap();
        assert_eq!(
            state
                .adopt_at(frame, [0.1, 40.0], Duration::from_secs(seconds))
                .unwrap()
                .unwrap()
                .depth,
            expected
        );
    }
}

#[test]
fn quantized_per_bound_uses_elapsed_time_after_outward_snapping() {
    let mut state = AdoptionState::new(config(AdoptionPolicy::QuantizedPerBound {
        latency_frames: 1,
        contraction_delay: std::time::Duration::from_millis(100),
    }))
    .unwrap();
    let r = |near, far| DepthRange::Receivers { near, far };
    let mut step = |frame, millis, depth| {
        state.complete(&state.ticket(frame - 1), depth).unwrap();
        state
            .adopt_at(frame, [0.1, 40.0], std::time::Duration::from_millis(millis))
            .unwrap()
            .unwrap()
            .depth
    };
    assert_eq!(step(1, 0, r(2.7, 12.2)), r(2.5, 13.0));
    assert_eq!(step(2, 10, r(3.7, 12.2)), r(2.5, 13.0));
    assert_eq!(step(3, 60, r(3.7, 10.2)), r(2.5, 13.0));
    assert_eq!(step(4, 110, r(3.7, 10.2)), r(3.5, 13.0));
    assert_eq!(step(5, 160, r(1.7, 10.2)), r(1.5, 11.0));
    assert_eq!(step(6, 170, r(2.7, 20.2)), r(1.5, 21.0));
    assert_eq!(step(7, 180, DepthRange::EmptyDepth), DepthRange::EmptyDepth);
}

use super::*;
fn config(policy: AdoptionPolicy) -> SdsmSettings {
    SdsmSettings {
        policy,
        range: RangeFit {
            near_step: 0.5,
            far_step: 1.0,
            minimum_span: 1.0,
            minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
            active_maximum: None,
            split_lambda: 0.5,
            split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
        },
    }
}
fn reading() -> DepthRange {
    DepthRange::Receivers {
        near: 2.7,
        far: 12.2,
    }
}
#[test]
fn exact_delayed_frame_identity_and_same_frame_capture() {
    let mut state = AdoptionState::new(config(AdoptionPolicy::Interactive {
        latency_frames: 2,
        contraction_frames: 3,
    }))
    .unwrap();
    let ticket = state.ticket(10);
    state.complete(&ticket, reading()).unwrap();
    assert_eq!(state.adopt(11, [0.1, 20.0]).unwrap(), None);
    let fit = state.adopt(12, [0.1, 20.0]).unwrap().unwrap();
    assert_eq!(fit.source_frame, 10);
    assert_eq!(fit.adopted_frame, 12);
    assert_eq!(
        fit.depth,
        DepthRange::Receivers {
            near: 2.5,
            far: 13.0
        }
    );
    let mut capture = AdoptionState::new(config(AdoptionPolicy::SameFrameCapture)).unwrap();
    capture.complete(&capture.ticket(42), reading()).unwrap();
    assert_eq!(
        capture
            .adopt(42, [0.1, 20.0])
            .unwrap()
            .unwrap()
            .source_frame,
        42
    );
}
#[test]
fn cancelled_prefix_has_no_result_and_resize_rejects_old_tickets() {
    let mut state = AdoptionState::new(config(AdoptionPolicy::SameFrameCapture)).unwrap();
    let ticket = state.ticket(7);
    assert_eq!(state.adopt(7, [0.1, 20.0]).unwrap(), None);
    state.invalidate();
    assert!(matches!(
        state.complete(&ticket, reading()),
        Err(SdsmError::StaleResource)
    ));
    assert_eq!(state.adopt(7, [0.1, 20.0]).unwrap(), None);
}
#[test]
fn malformed_result_does_not_reuse_a_previous_fit() {
    let mut state = AdoptionState::new(config(AdoptionPolicy::SameFrameCapture)).unwrap();
    state.complete(&state.ticket(1), reading()).unwrap();
    state.adopt(1, [0.1, 20.0]).unwrap().unwrap();
    assert!(
        state
            .complete(
                &state.ticket(2),
                DepthRange::Receivers {
                    near: f32::NAN,
                    far: 2.0
                }
            )
            .is_err()
    );
    assert_eq!(state.adopt(2, [0.1, 20.0]).unwrap(), None);
}
#[test]
fn reduction_kernel_validates() {
    let module = naga::front::wgsl::parse_str(include_str!("sdsm_reduce.wgsl")).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn interactive_adopts_late_results_at_next_opportunity() {
    let mut state = AdoptionState::new(config(AdoptionPolicy::Interactive {
        latency_frames: 2,
        contraction_frames: 0,
    }))
    .unwrap();
    assert_eq!(state.adopt(12, [0.1, 20.0]).unwrap(), None);
    state.complete(&state.ticket(10), reading()).unwrap();
    let fit = state.adopt(13, [0.1, 20.0]).unwrap().unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (10, 13));
    assert_eq!(state.adopt(14, [0.1, 20.0]).unwrap(), None);
}

#[test]
fn interactive_newest_eligible_wins_in_both_completion_orders() {
    for order in [[10, 11], [11, 10]] {
        let mut state = AdoptionState::new(config(AdoptionPolicy::Interactive {
            latency_frames: 2,
            contraction_frames: 0,
        }))
        .unwrap();
        for source in order {
            state.complete(&state.ticket(source), reading()).unwrap();
        }
        state.complete(&state.ticket(20), reading()).unwrap();
        let fit = state.adopt(15, [0.1, 30.0]).unwrap().unwrap();
        assert_eq!(fit.source_frame, 11);
        assert_eq!(state.adopt(16, [0.1, 30.0]).unwrap(), None);
        assert_eq!(
            state.adopt(22, [0.1, 30.0]).unwrap().unwrap().source_frame,
            20
        );
    }
}

#[test]
fn interactive_older_completion_after_adoption_cannot_regress() {
    let mut state = AdoptionState::new(config(AdoptionPolicy::Interactive {
        latency_frames: 2,
        contraction_frames: 0,
    }))
    .unwrap();
    state.complete(&state.ticket(11), reading()).unwrap();
    assert_eq!(
        state.adopt(13, [0.1, 20.0]).unwrap().unwrap().source_frame,
        11
    );
    state
        .complete(&state.ticket(10), DepthRange::EmptyDepth)
        .unwrap();
    assert_eq!(state.adopt(14, [0.1, 20.0]).unwrap(), None);
    state.complete(&state.ticket(12), reading()).unwrap();
    assert_eq!(
        state.adopt(15, [0.1, 20.0]).unwrap().unwrap().source_frame,
        12
    );
}
