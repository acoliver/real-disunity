use super::*;
use crate::{ViewerError, run_live};

struct NoStartup;
impl LiveDriver for NoStartup {
    fn initialize(&mut self, _: Viewport) -> Result<(), String> {
        panic!("invalid host options must fail before startup")
    }
    fn update(&mut self, _: &LiveTick) -> Result<LiveFrame, String> {
        panic!("invalid host options must fail before simulation")
    }
}

#[test]
fn invalid_live_dimensions_fail_without_starting_a_window_or_driver() {
    for size in [[0, 1], [1, 0], [4097, 64], [64, u32::MAX]] {
        assert!(matches!(
            run_live(&mut NoStartup, LiveOptions::new("test", size)),
            Err(ViewerError::InvalidOptions(_))
        ));
    }
}

#[test]
fn invalid_live_timing_and_frame_limit_fail_before_startup() {
    for interval in [Duration::ZERO, Duration::from_secs(2), Duration::MAX] {
        let mut options = LiveOptions::new("test", [64, 64]);
        options.frame_interval = interval;
        assert!(matches!(
            run_live(&mut NoStartup, options),
            Err(ViewerError::InvalidOptions(_))
        ));
    }
    let mut options = LiveOptions::new("test", [64, 64]);
    options.max_frames = Some(0);
    assert!(matches!(
        run_live(&mut NoStartup, options),
        Err(ViewerError::InvalidOptions(_))
    ));
}

#[test]
fn struct_literals_and_default_ingress_need_no_optional_fields() {
    let options = LiveOptions {
        title: "defaults".into(),
        size: [64, 64],
        frame_interval: Duration::from_millis(16),
        fixed_dt: None,
        max_frames: Some(1),
        scheduled_input: vec![],
        input: None,
    };
    let tick = LiveTick {
        frame: 0,
        elapsed: Duration::ZERO,
        dt: Duration::ZERO,
        viewport: Viewport {
            width: options.size[0],
            height: options.size[1],
        },
        input: vec![LiveInputEvent::CloseRequested],
    };
    let sample = LiveInputSample {
        sequence: 1,
        timestamp: tick.elapsed,
        origin: IngressOrigin::OsEventLoop,
        event: tick.input[0].clone(),
    };
    assert_eq!(NoStartup.ingress(&sample), Ok(()));
}

#[test]
fn acquisition_and_fixed_replay_clocks_reject_before_driver_initialization() {
    struct Acquisition;
    impl LiveDriver for Acquisition {
        fn clock(&self) -> LiveClock {
            LiveClock::Acquisition
        }
        fn initialize(&mut self, _: Viewport) -> Result<(), String> {
            panic!("invalid clock must reject before initialize")
        }
        fn update(&mut self, _: &LiveTick) -> Result<LiveFrame, String> {
            panic!("invalid clock must reject before update")
        }
    }
    let mut options = LiveOptions::new("invalid mixed timing", [64, 64]);
    options.fixed_dt = Some(Duration::from_millis(20));
    let result = run_live(&mut Acquisition, options);
    assert!(matches!(
        result,
        Err(ViewerError::InvalidOptions(
            "acquisition clock cannot use a fixed replay delta"
        ))
    ));
}

#[test]
fn ready_prewarmed_and_fixed_replay_clocks_reject_before_driver_initialization() {
    struct Prewarmed;
    impl LiveDriver for Prewarmed {
        fn clock(&self) -> LiveClock {
            LiveClock::ReadyPrewarmed
        }
        fn initialize(&mut self, _: Viewport) -> Result<(), String> {
            panic!("invalid clock must reject before initialize")
        }
        fn update(&mut self, _: &LiveTick) -> Result<LiveFrame, String> {
            panic!("invalid clock must reject before update")
        }
    }
    let mut options = LiveOptions::new("invalid prewarmed timing", [64, 64]);
    options.fixed_dt = Some(Duration::from_millis(20));
    let result = run_live(&mut Prewarmed, options);
    assert!(matches!(
        result,
        Err(ViewerError::InvalidOptions(
            "acquisition clock cannot use a fixed replay delta"
        ))
    ));
    assert!(LiveClock::ReadyPrewarmed.real_time());
    assert!(LiveClock::Acquisition.real_time());
    assert!(!LiveClock::FrameDelta.real_time());
}
