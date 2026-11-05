use super::*;
use crate::{CursorError, CursorGrabMode, RetainedEventHost, WindowControlError};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

#[derive(Debug, PartialEq)]
enum Call {
    Focus,
    Grab(CursorGrabMode),
    Visible(bool),
}

#[derive(Default)]
struct WindowControls(Mutex<Vec<Call>>);

impl super::super::input::WindowControls for WindowControls {
    fn focus_window(&self) {
        self.0.lock().unwrap().push(Call::Focus);
    }

    fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), winit::error::ExternalError> {
        self.0.lock().unwrap().push(Call::Grab(mode));
        Ok(())
    }

    fn set_cursor_visible(&self, visible: bool) {
        self.0.lock().unwrap().push(Call::Visible(visible));
    }
}

#[derive(Default)]
struct Driver(Rc<RefCell<Vec<RetainedEvent>>>);

impl RetainedDriver for Driver {
    fn update(&mut self, host: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        host.focus_window().unwrap();
        Ok(())
    }

    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.0.borrow_mut().push(event.clone());
        if matches!(event, RetainedEvent::Input(RetainedInput::Occluded(true))) {
            host.focus_window().unwrap();
            host.focus_window().unwrap();
        }
        Ok(())
    }
}

fn host() -> RetainedHost {
    RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap()
}

#[test]
fn focus_window_before_native_startup_returns_no_window_from_both_hosts() {
    let mut host = host();
    assert_eq!(host.focus_window(), Err(WindowControlError::NoWindow));
    let mut event_host = RetainedEventHost { host: &mut host };
    assert_eq!(event_host.focus_window(), Err(WindowControlError::NoWindow));
}

#[test]
fn focus_window_forwards_each_request_once_without_synthesizing_input() {
    let mut driver = Driver::default();
    let events = driver.0.clone();
    let mut session = Session::new(host(), &mut driver);
    let window = Arc::new(WindowControls::default());
    session.set_window_controls(window.clone());
    session.host.focus_window().unwrap();
    session.host.focus_window().unwrap();
    session.tick().unwrap();
    assert!(events.borrow().is_empty());
    assert_eq!(
        *window.0.lock().unwrap(),
        [Call::Focus, Call::Focus, Call::Focus]
    );
    session.event(&WindowEvent::Occluded(true), None);
    assert_eq!(
        *window.0.lock().unwrap(),
        [
            Call::Focus,
            Call::Focus,
            Call::Focus,
            Call::Focus,
            Call::Focus
        ]
    );
    assert_eq!(
        *events.borrow(),
        [RetainedEvent::Input(RetainedInput::Occluded(true))]
    );
    session.event(&WindowEvent::Focused(true), None);
    assert_eq!(
        *events.borrow(),
        [
            RetainedEvent::Input(RetainedInput::Occluded(true)),
            RetainedEvent::Input(RetainedInput::Focused(true))
        ]
    );
}

#[test]
fn focus_window_and_cursor_controls_share_the_session_window_seam() {
    let mut driver = Driver::default();
    let mut session = Session::new(host(), &mut driver);
    let window = Arc::new(WindowControls::default());
    session.set_window_controls(window.clone());
    session
        .host
        .set_cursor_grab(CursorGrabMode::Confined)
        .unwrap();
    session.host.set_cursor_visible(false).unwrap();
    let mut event_host = RetainedEventHost {
        host: &mut session.host,
    };
    event_host.set_cursor_grab(CursorGrabMode::None).unwrap();
    event_host.set_cursor_visible(true).unwrap();
    event_host.focus_window().unwrap();
    assert_eq!(
        *window.0.lock().unwrap(),
        [
            Call::Grab(CursorGrabMode::Confined),
            Call::Visible(false),
            Call::Grab(CursorGrabMode::None),
            Call::Visible(true),
            Call::Focus
        ]
    );
}

#[test]
fn focus_window_is_unsupported_in_windowless_startup_without_a_gpu() {
    host()
        .start_windowless_with_activity(
            &mut Driver::default(),
            |_| (),
            |mut host, _| {
                assert_eq!(host.focus_window(), Err(WindowControlError::Unsupported));
                let mut event_host = RetainedEventHost { host: &mut host };
                assert_eq!(
                    event_host.focus_window(),
                    Err(WindowControlError::Unsupported)
                );
                assert!(matches!(
                    event_host.set_cursor_visible(false),
                    Err(CursorError::Unavailable)
                ));
                assert!(matches!(
                    event_host.set_cursor_grab(CursorGrabMode::Locked),
                    Err(CursorError::Unavailable)
                ));
                Ok(())
            },
        )
        .unwrap();
}
