#[path = "strict.rs"]
mod strict;

use super::*;
use crate::live::{IngressOrigin, LiveInputSample, ScheduledInput};

#[derive(Default)]
struct Driver {
    calls: Vec<&'static str>,
    samples: Vec<LiveInputSample>,
    ticks: Vec<LiveTick>,
    reject: bool,
    reject_at: Option<u64>,
    clock: super::super::LiveClock,
}

impl LiveDriver for Driver {
    fn clock(&self) -> super::super::LiveClock {
        self.clock
    }

    fn initialize(&mut self, _: Viewport) -> Result<(), String> {
        self.calls.push("initialize");
        Ok(())
    }

    fn ingress(&mut self, sample: &LiveInputSample) -> Result<(), String> {
        self.calls.push("ingress");
        self.samples.push(sample.clone());
        if self.reject || self.reject_at == Some(sample.sequence) {
            return Err("ingress sentinel".into());
        }
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        self.calls.push("update");
        self.ticks.push(tick.clone());
        Err("update sentinel".into())
    }
}

fn viewport() -> Viewport {
    Viewport {
        width: 64,
        height: 64,
    }
}

fn options() -> LiveOptions {
    LiveOptions::new("ingress test", [64, 64])
}

#[test]
fn preinitialize_transitions_are_observed_before_update_without_changing_tick_input() {
    let mut driver = Driver::default();
    let mut session = Session::new(&mut driver, options());
    session.event(&WindowEvent::Focused(false), None);
    session.event(&WindowEvent::Focused(true), None);
    session.next_frame(viewport()).err().unwrap();
    assert_eq!(driver.calls, ["initialize", "ingress", "ingress", "update"]);
    assert_eq!(driver.samples.len(), 2);
    for (index, sample) in driver.samples.iter().enumerate() {
        assert_eq!(sample.sequence, u64::try_from(index).unwrap() + 1);
        assert_eq!(sample.timestamp, Duration::ZERO);
        assert_eq!(sample.origin, IngressOrigin::OsEventLoop);
        assert_eq!(sample.event, driver.ticks[0].input[index]);
    }
}

#[test]
fn replay_is_logical_and_channel_is_explicit_delivery_time_in_arrival_order() {
    let mut options = options();
    options.fixed_dt = Some(Duration::from_millis(20));
    options.scheduled_input = vec![ScheduledInput {
        frame: 3,
        event: LiveInputEvent::CaptureRequested(9),
    }];
    let (sender, receiver) = std::sync::mpsc::channel();
    options.input = Some(receiver);
    sender.send(LiveInputEvent::Text("channel".into())).unwrap();
    let mut driver = Driver::default();
    let mut session = Session::new(&mut driver, options);
    session.frame = 3;
    session.event(&WindowEvent::Focused(false), None);
    session.next_frame(viewport()).err().unwrap();
    assert_eq!(driver.samples.len(), 3);
    assert_eq!(driver.samples[0].timestamp, Duration::from_millis(60));
    assert_eq!(driver.samples[0].origin, IngressOrigin::ScheduledReplay);
    assert_eq!(driver.samples[1].origin, IngressOrigin::OsEventLoop);
    assert_eq!(driver.samples[2].origin, IngressOrigin::ChannelDelivery);
    assert_eq!(
        driver.ticks[0].input,
        driver
            .samples
            .iter()
            .map(|s| s.event.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn ingress_error_aborts_before_update_and_retains_driver_error() {
    let mut driver = Driver {
        reject: true,
        ..Driver::default()
    };
    let mut session = Session::new(&mut driver, options());
    session.event(&WindowEvent::CloseRequested, None);
    let result = session.next_frame(viewport());
    assert!(matches!(result, Err(ViewerError::Driver(error)) if error == "ingress sentinel"));
    assert_eq!(driver.calls, ["initialize", "ingress"]);
    assert!(driver.ticks.is_empty());
}

#[test]
fn os_burst_over_budget_fails_before_any_ingress_or_update() {
    let mut driver = Driver::default();
    let mut session = Session::new(&mut driver, options());
    for _ in 0..4097 {
        session.event(&WindowEvent::Focused(false), None);
    }
    let result = session.next_frame(viewport());
    assert!(matches!(result, Err(ViewerError::Driver(error)) if error.contains("4096")));
    assert_eq!(driver.calls, ["initialize"]);
}

#[test]
fn ingress_failure_stops_remaining_observations_without_half_frame_update() {
    let mut driver = Driver {
        reject_at: Some(2),
        ..Driver::default()
    };
    let mut session = Session::new(&mut driver, options());
    for focused in [true, false, true] {
        session.event(&WindowEvent::Focused(focused), None);
    }
    let result = session.next_frame(viewport());
    assert!(matches!(result, Err(ViewerError::Driver(error)) if error == "ingress sentinel"));
    assert_eq!(driver.calls, ["initialize", "ingress", "ingress"]);
    assert!(driver.ticks.is_empty());
}

#[test]
fn first_acquisition_cutoff_includes_already_acquired_os_samples() {
    let mut driver = Driver {
        clock: super::super::LiveClock::Acquisition,
        ..Driver::default()
    };
    let mut session = Session::new(&mut driver, options());
    let origin = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    session.start_clock(origin);
    session.initialized = true;
    session.input.push(
        LiveInputEvent::Focused(true),
        origin + Duration::from_millis(500),
    );
    let tick = session.tick(viewport()).unwrap();
    assert!(tick.elapsed + tick.dt >= Duration::from_millis(500));
}

#[test]
fn acquisition_intervals_conserve_exact_nanoseconds_and_match_os_cutoffs() {
    let mut driver = Driver {
        clock: super::super::LiveClock::Acquisition,
        ..Driver::default()
    };
    let mut session = Session::new(&mut driver, options());
    let origin = Instant::now();
    session.start_clock(origin);
    let mut total = Duration::ZERO;
    for nanos in [1, 16_666_666, 3, 8_000_001] {
        let dt = Duration::from_nanos(nanos);
        total += dt;
        let endpoint = origin + total;
        session.input.push(LiveInputEvent::Focused(true), endpoint);
        let tick = session.tick_at(viewport(), endpoint).unwrap();
        assert_eq!(tick.dt, dt);
        assert_eq!(tick.elapsed + tick.dt, total);
        session.elapsed += tick.dt;
    }
    assert_eq!(driver.samples.len(), 4);
    assert_eq!(driver.samples.last().unwrap().timestamp, total);
}

#[test]
fn default_frame_delta_clock_keeps_the_first_zero_delta() {
    let mut driver = Driver::default();
    let mut session = Session::new(&mut driver, options());
    let origin = Instant::now();
    session.start_clock(origin);
    let first = session
        .tick_at(viewport(), origin + Duration::from_secs(1))
        .unwrap();
    assert_eq!(first.dt, Duration::ZERO);
    let second = session
        .tick_at(viewport(), origin + Duration::from_secs(2))
        .unwrap();
    assert_eq!(second.dt, Duration::from_secs(1));
}
