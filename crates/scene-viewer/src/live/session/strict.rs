use super::*;

#[derive(Default)]
struct StrictDriver {
    replay: Option<bool>,
    updates: usize,
}

impl LiveDriver for StrictDriver {
    fn ingress(&mut self, sample: &LiveInputSample) -> Result<(), String> {
        let replay = sample.origin == IngressOrigin::ScheduledReplay;
        if self.replay.is_some_and(|previous| previous != replay) {
            return Err("strict adapter requires one timestamp domain".into());
        }
        self.replay = Some(replay);
        Ok(())
    }

    fn update(&mut self, _: &LiveTick) -> Result<LiveFrame, String> {
        self.updates += 1;
        Err("update sentinel".into())
    }
}

#[test]
fn strict_consumer_rejects_mixed_replay_and_host_clocks_before_update() {
    let mut options = options();
    options.fixed_dt = Some(Duration::from_secs(1));
    options.scheduled_input.push(ScheduledInput {
        frame: 3,
        event: LiveInputEvent::CaptureRequested(1),
    });
    let mut driver = StrictDriver::default();
    let mut session = Session::new(&mut driver, options);
    session.frame = 3;
    session.event(&WindowEvent::Focused(true), None);
    let result = session.next_frame(viewport());
    assert!(
        matches!(result, Err(ViewerError::Driver(error)) if error == "strict adapter requires one timestamp domain")
    );
    assert_eq!(driver.updates, 0);
}

#[test]
fn strict_consumer_accepts_os_and_channel_as_one_host_acquisition_domain() {
    let mut options = options();
    let (sender, receiver) = std::sync::mpsc::channel();
    options.input = Some(receiver);
    sender.send(LiveInputEvent::CaptureRequested(1)).unwrap();
    let mut driver = StrictDriver::default();
    let mut session = Session::new(&mut driver, options);
    session.event(&WindowEvent::Focused(true), None);
    let result = session.next_frame(viewport());
    assert!(matches!(result, Err(ViewerError::Driver(error)) if error == "update sentinel"));
    assert_eq!(driver.updates, 1);
}

#[test]
fn overdue_replay_keeps_stable_frame_order_and_uses_configured_interval_without_fixed_dt() {
    let mut options = options();
    options.frame_interval = Duration::from_nanos(7);
    for (frame, id) in [(2, 1), (0, 2), (2, 3)] {
        options.scheduled_input.push(ScheduledInput {
            frame,
            event: LiveInputEvent::CaptureRequested(id),
        });
    }
    let mut driver = Driver::default();
    let mut session = Session::new(&mut driver, options);
    session.frame = 2;
    session.next_frame(viewport()).err().unwrap();
    assert_eq!(
        driver
            .samples
            .iter()
            .map(|s| s.timestamp)
            .collect::<Vec<_>>(),
        [
            Duration::ZERO,
            Duration::from_nanos(14),
            Duration::from_nanos(14)
        ]
    );
    assert_eq!(
        driver.ticks[0].input,
        [
            LiveInputEvent::CaptureRequested(2),
            LiveInputEvent::CaptureRequested(1),
            LiveInputEvent::CaptureRequested(3)
        ]
    );
}

#[test]
fn channel_over_budget_is_rejected_before_any_callback() {
    let mut options = options();
    let (sender, receiver) = std::sync::mpsc::channel();
    options.input = Some(receiver);
    for _ in 0..4097 {
        sender.send(LiveInputEvent::Focused(true)).unwrap();
    }
    let mut driver = Driver::default();
    let mut session = Session::new(&mut driver, options);
    let result = session.next_frame(viewport());
    assert!(matches!(result, Err(ViewerError::Driver(error)) if error.contains("4096")));
    assert_eq!(driver.calls, ["initialize"]);
}
