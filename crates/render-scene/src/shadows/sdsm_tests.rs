use super::*;
fn config(policy: AdoptionPolicy) -> SdsmSettings {
    SdsmSettings {
        policy,
        range: RangeFit {
            near_step: 0.5,
            far_step: 1.0,
            minimum_span: 1.0,
            split_lambda: 0.5,
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
