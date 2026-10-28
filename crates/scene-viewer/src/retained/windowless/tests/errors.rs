use super::*;
use crate::{RetainedEventHost, RetainedInput, retained::session::drop_tests};

#[derive(Clone, Copy)]
enum Failure {
    Resize,
    Update,
    Completion,
    Shutdown,
    Cancellation,
}
struct Failing {
    failure: Failure,
    events: Rc<RefCell<Vec<RetainedEvent>>>,
}
impl Failing {
    fn new(failure: Failure) -> Self {
        Self {
            failure,
            events: Rc::default(),
        }
    }
}
impl RetainedDriver for Failing {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        if matches!(self.failure, Failure::Update) {
            return Err("update-first".into());
        }
        Ok(())
    }
    fn event(
        &mut self,
        _: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.events.borrow_mut().push(event.clone());
        match event {
            RetainedEvent::Input(RetainedInput::Resized(_))
                if matches!(self.failure, Failure::Resize) =>
            {
                Err("resize-first".into())
            }
            RetainedEvent::Capture {
                outcome: RetainedCaptureOutcome::Completed { .. },
                ..
            } if matches!(self.failure, Failure::Completion) => Err("completion-first".into()),
            RetainedEvent::Capture {
                outcome: RetainedCaptureOutcome::Cancelled,
                ..
            } if !matches!(self.failure, Failure::Shutdown) => Err("capture-cleanup".into()),
            RetainedEvent::Shutdown => Err("shutdown-cleanup".into()),
            _ => Ok(()),
        }
    }
}
fn assert_cancelled_then_shutdown(events: &[RetainedEvent], ids: &[u64]) {
    let cleanup = &events[events.len() - ids.len() - 1..];
    for (event, id) in cleanup.iter().zip(ids) {
        assert!(matches!(event, RetainedEvent::Capture {
            id: actual, outcome: RetainedCaptureOutcome::Cancelled, ..
        } if actual == id));
    }
    assert_eq!(cleanup.last(), Some(&RetainedEvent::Shutdown));
    assert_eq!(
        events
            .iter()
            .filter(|e| **e == RetainedEvent::Shutdown)
            .count(),
        1
    );
}
#[test]
fn resize_and_update_errors_survive_failing_cleanup() {
    for (failure, message) in [
        (Failure::Resize, "resize-first"),
        (Failure::Update, "update-first"),
    ] {
        let mut driver = Failing::new(failure);
        let events = driver.events.clone();
        let mut run = host(crate::renderer::retained_tests::source())
            .windowless(&mut driver)
            .unwrap();
        let ids = [
            run.host_mut().request_capture(0).unwrap(),
            run.host_mut().request_capture(0).unwrap(),
        ];
        let result = if matches!(failure, Failure::Resize) {
            run.resize([0, 0])
        } else {
            run.frame().map(|_| ())
        };
        assert!(matches!(result, Err(ViewerError::Driver(e)) if e == message));
        assert!(run.closed);
        drop(run);
        assert_cancelled_then_shutdown(&events.borrow(), &ids);
    }
}
#[test]
fn completion_error_survives_failing_cleanup() {
    let mut driver = Failing::new(Failure::Completion);
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let completed = run.host_mut().request_capture(0).unwrap();
    assert!(run.frame().unwrap());
    let cancelled = run.host_mut().request_capture(0).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let error = loop {
        if let Err(error) = run.poll() {
            break error;
        }
        assert!(Instant::now() < deadline, "completion timed out");
        std::thread::yield_now();
    };
    assert!(matches!(error, ViewerError::Driver(e) if e == "completion-first"));
    assert!(run.closed);
    drop(run);
    let events = events.borrow();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, RetainedEvent::Capture {
        id, outcome: RetainedCaptureOutcome::Completed { .. }, ..
    } if *id == completed))
    );
    assert_cancelled_then_shutdown(&events, &[cancelled]);
}
#[test]
fn construction_error_survives_failing_cleanup() {
    let mut driver = Failing::new(Failure::Cancellation);
    let events = driver.events.clone();
    let mut host = host(crate::renderer::retained_tests::source());
    let ids = [
        host.request_capture(0).unwrap(),
        host.request_capture(0).unwrap(),
    ];
    host.options.size = [u32::MAX, 64];
    let result = host.windowless(&mut driver);
    assert!(matches!(
        result,
        Err(ViewerError::SurfaceExtent {
            size: [u32::MAX, 64],
            ..
        })
    ));
    drop(result);
    assert_cancelled_then_shutdown(&events.borrow(), &ids);
}
#[test]
fn successful_operation_propagates_shutdown_failure() {
    let mut driver = Failing::new(Failure::Shutdown);
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    run.host_mut().request_exit();
    assert!(matches!(run.poll(), Err(ViewerError::Driver(e)) if e == "shutdown-cleanup"));
    drop(run);
    assert_cancelled_then_shutdown(&events.borrow(), &[]);
}
#[test]
fn drop_reports_cleanup_failure_without_panicking() {
    let mut driver = Failing::new(Failure::Cancellation);
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let id = run.host_mut().request_capture(0).unwrap();
    assert!(run.frame().unwrap());
    drop(run);
    assert_cancelled_then_shutdown(&events.borrow(), &[id]);
}
#[test]
fn shutdown_errors_during_unwind_preserve_the_original_panic() {
    if let Ok(scenario) = std::env::var(drop_tests::CHILD) {
        unwind_child(&scenario);
    }
    for scenario in ["capture", "shutdown"] {
        drop_tests::assert_panic_exit(
            concat!(
                module_path!(),
                "::shutdown_errors_during_unwind_preserve_the_original_panic"
            ),
            scenario,
        );
    }
}
fn unwind_child(scenario: &str) -> ! {
    let failure = if scenario == "capture" {
        Failure::Cancellation
    } else {
        Failure::Shutdown
    };
    let mut driver = Failing::new(failure);
    let events = driver.events.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut run = host(crate::renderer::retained_tests::source())
            .windowless(&mut driver)
            .unwrap();
        run.host_mut().request_capture(0).unwrap();
        assert!(run.frame().unwrap());
        run.host_mut().request_capture(0).unwrap();
        panic!("original application panic");
    }));
    assert_cancelled_then_shutdown(&events.borrow(), &[0, 1]);
    println!("retained unwind cleanup verified");
    std::panic::resume_unwind(result.unwrap_err());
}
