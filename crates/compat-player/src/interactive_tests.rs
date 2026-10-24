use super::{Config, Continuous, MAX_STEP};
use std::time::Duration;
use worldrunner::scene::{gpu::Viewport, viewer::LiveTick};

fn clock(dt: Duration, elapsed: Duration, frame: u64) -> LiveTick {
    LiveTick {
        dt,
        elapsed,
        frame,
        viewport: Viewport {
            width: 960,
            height: 720,
        },
        input: vec![],
    }
}

fn continuous() -> Continuous {
    Continuous::new(Config {
        duration_ms: 30000,
        max_frames: 1800,
        max_captures: 8,
    })
}

#[test]
fn live_delta_is_preserved_in_steps_no_larger_than_one_sixtieth() {
    let mut state = continuous();
    let steps = state.steps(&clock(Duration::from_millis(47), Duration::ZERO, 0));
    assert!(steps.iter().all(|dt| *dt > 0.0 && *dt <= MAX_STEP));
    assert!((steps.iter().sum::<f64>() - 0.047).abs() < 1e-12);
    assert!((state.simulated() - 0.047).abs() < 1e-12);
    assert!(
        state
            .steps(&clock(Duration::ZERO, Duration::ZERO, 1))
            .is_empty()
    );
}

#[test]
fn stall_recovery_has_a_fixed_work_bound() {
    let mut state = continuous();
    let tick = clock(Duration::from_secs(20), Duration::ZERO, 0);
    let steps = state.steps(&tick);
    assert!(steps.len() <= 15);
    assert!((steps.iter().sum::<f64>() - 0.25).abs() < 1e-12);
    assert!(!state.finished(&tick));
    assert!(state.finished(&clock(Duration::from_secs(30), Duration::ZERO, 0)));
}

#[test]
fn duration_and_frame_budgets_are_independent() {
    let mut state = continuous();
    let last = clock(Duration::from_millis(20), Duration::from_millis(29995), 2);
    assert!(state.finished(&last));
    assert!((state.steps(&last).iter().sum::<f64>() - 0.005).abs() < 1e-12);
    assert!(state.finished(&clock(Duration::ZERO, Duration::ZERO, 1799)));
    assert!(!state.finished(&clock(Duration::ZERO, Duration::ZERO, 1798)));
    assert!(
        state
            .steps(&clock(Duration::from_secs(1), Duration::from_secs(31), 3))
            .is_empty()
    );
}
