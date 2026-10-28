use super::*;
use std::{cell::RefCell, rc::Rc};

struct Consumer(Rc<RefCell<Vec<RetainedEvent>>>);
impl RetainedDriver for Consumer {
    fn update(&mut self, _host: &mut RetainedHost, _tick: &RetainedTick) -> Result<(), String> {
        Ok(())
    }
    fn event(
        &mut self,
        _host: &mut crate::RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.0.borrow_mut().push(event.clone());
        Ok(())
    }
}

#[test]
fn lifecycle_and_focus_events_reach_the_consumer_without_a_window() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut consumer = Consumer(events.clone());
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut session = Session::new(host, &mut consumer);
    session.input(RetainedInput::Key {
        key: "w".into(),
        logical: crate::LogicalKey::Character("w".into()),
        physical: crate::KeyCode::KeyW.into(),
        pressed: true,
        repeat: false,
    });
    session.event(&WindowEvent::Focused(false), None);
    session.surface_size([0, 0]);
    assert!(session.host.view().unwrap().is_none());
    session.surface_size([640, 480]);
    session.input(RetainedInput::SurfaceRecreated);
    assert_eq!(session.host.size(), [640, 480]);
    let events = events.borrow();
    assert_eq!(events.len(), 6);
    assert_eq!(
        events[1],
        RetainedEvent::Input(RetainedInput::Key {
            key: "w".into(),
            logical: crate::LogicalKey::Character("w".into()),
            physical: crate::KeyCode::KeyW.into(),
            pressed: false,
            repeat: false,
        })
    );
    assert_eq!(
        events[2],
        RetainedEvent::Input(RetainedInput::Focused(false))
    );
    assert_eq!(
        events[5],
        RetainedEvent::Input(RetainedInput::SurfaceRecreated)
    );
}

#[test]
fn input_callback_error_is_available_without_a_redraw() {
    struct Failing;
    impl RetainedDriver for Failing {
        fn update(&mut self, _host: &mut RetainedHost, _tick: &RetainedTick) -> Result<(), String> {
            Ok(())
        }
        fn event(
            &mut self,
            _host: &mut crate::RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            if matches!(event, RetainedEvent::Shutdown) {
                return Ok(());
            }
            Err("input failure".into())
        }
    }
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut driver = Failing;
    let mut session = Session::new(host, &mut driver);
    session.surface_size([0, 0]);
    assert!(
        matches!(session.take_failure(), Some(ViewerError::Driver(error)) if error == "input failure")
    );
}

#[test]
fn shutdown_delivers_cancellation_for_waiting_and_dispatched_requests() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut driver = Consumer(events.clone());
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let first = host.request_capture(0).unwrap();
    host.apply(crate::RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: crate::tests::options().pose,
        lens: crate::tests::options().camera,
    })
    .unwrap();
    let second = host.request_capture(1).unwrap();
    host.captures.waiting.pop_front();
    let mut session = Session::new(host, &mut driver);
    session.shutdown().unwrap();
    session.shutdown().unwrap();
    let events = events.borrow();
    assert_eq!(events.len(), 3);
    assert_eq!(events[2], RetainedEvent::Shutdown);
    for (event, id, generation) in [(&events[0], first, 0), (&events[1], second, 1)] {
        assert_eq!(
            *event,
            RetainedEvent::Capture {
                id,
                generation,
                outcome: crate::RetainedCaptureOutcome::Cancelled,
            }
        );
    }
}
