use super::*;
use crate::{CursorError, CursorGrabMode, RetainedCaptureOutcome, RetainedEventHost};
use std::{cell::RefCell, rc::Rc};

struct Driver {
    events: Rc<RefCell<Vec<RetainedEvent>>>,
    reject: Option<&'static str>,
}
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        panic!("lifecycle delivery must not need a frame");
    }
    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.events.borrow_mut().push(event.clone());
        match event {
            RetainedEvent::Input(
                RetainedInput::Occluded(true) | RetainedInput::Resized([0, 0]),
            ) => {
                assert!(matches!(
                    host.set_cursor_grab(CursorGrabMode::None),
                    Err(CursorError::Unavailable)
                ));
                assert!(matches!(
                    host.set_cursor_visible(true),
                    Err(CursorError::Unavailable)
                ));
                host.request_exit();
            }
            RetainedEvent::Capture { .. } if self.reject == Some("capture") => {
                return Err("capture failed".into());
            }
            RetainedEvent::Shutdown if self.reject == Some("shutdown") => {
                return Err("shutdown failed".into());
            }
            _ => {}
        }
        Ok(())
    }
}
fn driver() -> Driver {
    Driver {
        events: Rc::default(),
        reject: None,
    }
}
fn host() -> RetainedHost {
    RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap()
}
#[test]
fn paused_events_apply_exit_and_cursor_controls_without_updates() {
    for input in [
        RetainedInput::Occluded(true),
        RetainedInput::Resized([0, 0]),
    ] {
        let mut driver = driver();
        let mut session = Session::new(host(), &mut driver);
        session.input(input);
        assert!(session.exit_requested());
        assert!(session.host.view().unwrap().is_none());
        assert!(session.take_failure().is_none());
    }
}
#[test]
fn close_and_drop_notify_once_with_or_without_outstanding_captures() {
    for outstanding in [false, true] {
        for explicit in [false, true] {
            let mut driver = driver();
            let events = driver.events.clone();
            let mut host = host();
            if outstanding {
                host.request_capture(0).unwrap();
            }
            let mut session = Session::new(host, &mut driver);
            session.input(RetainedInput::CloseRequested);
            if explicit {
                session.shutdown().unwrap();
                session.shutdown().unwrap();
            }
            drop(session);
            let events = events.borrow();
            assert_eq!(events.last(), Some(&RetainedEvent::Shutdown));
            assert_eq!(
                events
                    .iter()
                    .filter(|e| **e == RetainedEvent::Shutdown)
                    .count(),
                1
            );
            assert_eq!(events.len(), if outstanding { 3 } else { 2 });
        }
    }
}
#[test]
fn cancellation_errors_do_not_skip_remaining_cancellations_or_shutdown() {
    let mut driver = driver();
    driver.reject = Some("capture");
    let events = driver.events.clone();
    let mut host = host();
    let first = host.request_capture(0).unwrap();
    let second = host.request_capture(0).unwrap();
    let mut session = Session::new(host, &mut driver);
    assert!(matches!(session.shutdown(), Err(ViewerError::Driver(e)) if e == "capture failed"));
    session.shutdown().unwrap();
    drop(session);
    assert_eq!(
        *events.borrow(),
        vec![
            RetainedEvent::Capture {
                id: first,
                generation: 0,
                outcome: RetainedCaptureOutcome::Cancelled
            },
            RetainedEvent::Capture {
                id: second,
                generation: 0,
                outcome: RetainedCaptureOutcome::Cancelled
            },
            RetainedEvent::Shutdown,
        ]
    );
}
#[test]
fn final_notification_error_is_returned_without_repeating_notification() {
    let mut driver = driver();
    driver.reject = Some("shutdown");
    let events = driver.events.clone();
    let mut session = Session::new(host(), &mut driver);
    assert!(matches!(session.shutdown(), Err(ViewerError::Driver(e)) if e == "shutdown failed"));
    session.shutdown().unwrap();
    drop(session);
    assert_eq!(*events.borrow(), vec![RetainedEvent::Shutdown]);
}

#[test]
fn drop_reports_a_final_callback_error_without_panicking_or_repeating_notification() {
    let mut driver = driver();
    driver.reject = Some("shutdown");
    let events = driver.events.clone();
    let session = Session::new(host(), &mut driver);
    drop(session);
    assert_eq!(*events.borrow(), vec![RetainedEvent::Shutdown]);
}

#[test]
fn shutdown_errors_during_unwind_preserve_the_original_panic() {
    use super::drop_tests::{CHILD, assert_panic_exit};
    if let Ok(scenario) = std::env::var(CHILD) {
        let mut driver = driver();
        driver.reject = Some(if scenario == "capture" {
            "capture"
        } else {
            "shutdown"
        });
        let events = driver.events.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut host = host();
            host.request_capture(0).unwrap();
            host.request_capture(0).unwrap();
            let _session = Session::new(host, &mut driver);
            panic!("original application panic");
        }));
        let events = events.borrow();
        assert_eq!(events.len(), 3);
        assert!(events[..2].iter().all(|event| matches!(
            event,
            RetainedEvent::Capture {
                outcome: RetainedCaptureOutcome::Cancelled,
                ..
            }
        )));
        assert_eq!(events.last(), Some(&RetainedEvent::Shutdown));
        println!("retained unwind cleanup verified");
        std::panic::resume_unwind(result.unwrap_err());
    }
    for scenario in ["capture", "shutdown"] {
        assert_panic_exit(
            concat!(
                module_path!(),
                "::shutdown_errors_during_unwind_preserve_the_original_panic"
            ),
            scenario,
        );
    }
}
