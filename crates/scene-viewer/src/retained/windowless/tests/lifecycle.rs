use super::*;
use crate::{RetainedEventHost, RetainedInput};

#[derive(Default)]
struct ExitOnCapture {
    events: Rc<RefCell<Vec<RetainedEvent>>>,
    updates: Rc<RefCell<u32>>,
}
impl RetainedDriver for ExitOnCapture {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        *self.updates.borrow_mut() += 1;
        Ok(())
    }
    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.events.borrow_mut().push(event.clone());
        if matches!(
            event,
            RetainedEvent::Capture {
                outcome: RetainedCaptureOutcome::Completed { .. },
                ..
            }
        ) {
            host.request_exit();
        }
        Ok(())
    }
}
#[test]
fn completion_while_paused_exits_through_poll_without_another_update() {
    for zero_extent in [false, true] {
        let mut driver = ExitOnCapture::default();
        let events = driver.events.clone();
        let updates = driver.updates.clone();
        let mut run = host(crate::renderer::retained_tests::source())
            .windowless(&mut driver)
            .unwrap();
        let id = run.host_mut().request_capture(0).unwrap();
        assert!(run.frame().unwrap());
        if zero_extent {
            run.resize([0, 0]).unwrap();
        } else {
            run.host_mut().input(crate::RetainedInput::Occluded(true));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while !run.closed {
            run.poll().unwrap();
            assert!(Instant::now() < deadline, "capture exit timed out");
            std::thread::yield_now();
        }
        assert_eq!(*updates.borrow(), 1);
        assert_eq!(run.session.frame, 1);
        let events = events.borrow();
        assert!(events.iter().any(|e| matches!(e, RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Completed { .. }, .. } if *actual == id)));
        assert_eq!(events.last(), Some(&RetainedEvent::Shutdown));
        assert_eq!(
            events
                .iter()
                .filter(|e| **e == RetainedEvent::Shutdown)
                .count(),
            1
        );
    }
}
#[test]
fn external_exit_and_explicit_shutdown_notify_once_without_frames() {
    for external in [false, true] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut run = host(crate::renderer::retained_tests::source())
            .windowless(&mut driver)
            .unwrap();
        if external {
            run.host_mut().request_exit();
            assert!(!run.poll().unwrap());
        } else {
            run.shutdown().unwrap();
        }
        run.shutdown().unwrap();
        drop(run);
        assert_eq!(*events.borrow(), vec![RetainedEvent::Shutdown]);
    }
}
#[test]
fn zero_extent_and_restored_resize_are_delivered_without_frames() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    run.resize([0, 0]).unwrap();
    assert!(!run.frame().unwrap());
    run.resize([32, 48]).unwrap();
    assert_eq!(run.host().size(), [32, 48]);
    assert_eq!(run.session.frame, 0);
    run.shutdown().unwrap();
    assert_eq!(
        *events.borrow(),
        vec![
            RetainedEvent::Input(RetainedInput::Resized([0, 0])),
            RetainedEvent::Input(RetainedInput::Resized([32, 48])),
            RetainedEvent::Shutdown,
        ]
    );
}
#[test]
fn resize_callback_failure_closes_and_delivers_final_notification() {
    struct FailResize(Rc<RefCell<Vec<RetainedEvent>>>);
    impl RetainedDriver for FailResize {
        fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
            Ok(())
        }
        fn event(
            &mut self,
            _: &mut RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            self.0.borrow_mut().push(event.clone());
            if matches!(event, RetainedEvent::Input(RetainedInput::Resized(_))) {
                return Err("resize failed".into());
            }
            Ok(())
        }
    }
    let events: Rc<RefCell<Vec<RetainedEvent>>> = Rc::default();
    let mut driver = FailResize(events.clone());
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    assert!(matches!(run.resize([0, 0]), Err(ViewerError::Driver(e)) if e == "resize failed"));
    assert!(run.closed);
    drop(run);
    assert_eq!(events.borrow().last(), Some(&RetainedEvent::Shutdown));
}

#[test]
fn drop_without_captures_notifies_once() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    drop(run);
    assert_eq!(*events.borrow(), vec![RetainedEvent::Shutdown]);
}

#[test]
fn external_exit_cancels_undelivered_capture_before_polling() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let id = run.host_mut().request_capture(0).unwrap();
    run.host_mut().request_exit();
    assert!(!run.poll().unwrap());
    drop(run);
    assert_eq!(
        *events.borrow(),
        vec![
            RetainedEvent::Capture {
                id,
                generation: 0,
                outcome: RetainedCaptureOutcome::Cancelled
            },
            RetainedEvent::Shutdown,
        ]
    );
}
