use super::*;
use crate::{KeyCode, MouseButton, PhysicalKey, WheelDelta};
use std::{cell::RefCell, rc::Rc};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseScrollDelta, TouchPhase},
    keyboard::Key,
};

struct Consumer(Rc<RefCell<Vec<RetainedEvent>>>);
impl RetainedDriver for Consumer {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        Ok(())
    }
    fn event(&mut self, event: &RetainedEvent) -> Result<(), String> {
        self.0.borrow_mut().push(event.clone());
        Ok(())
    }
}

#[test]
fn native_pointer_translation_preserves_delivery_order_and_units() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut consumer = Consumer(events.clone());
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut session = Session::new(host, &mut consumer);
    pointer_sequence(&mut session);
    let key = super::super::input::key_input(
        &Key::Character("Z".into()),
        KeyCode::KeyW.into(),
        ElementState::Pressed,
        true,
    );
    session.input(key.clone());
    let expected = vec![
        RetainedInput::CursorEntered,
        RetainedInput::PointerMoved([12.5, 80.25]),
        RetainedInput::Button {
            button: MouseButton::Other(7),
            pressed: true,
        },
        RetainedInput::Wheel(WheelDelta::Lines([1.5, -2.0])),
        RetainedInput::Wheel(WheelDelta::Pixels([0.5, -8.25])),
        RetainedInput::RawMotion([3.25, -7.5]),
        RetainedInput::CursorLeft,
        RetainedInput::Key {
            key: "z".into(),
            logical: crate::LogicalKey::Character("Z".into()),
            physical: KeyCode::KeyW.into(),
            pressed: true,
            repeat: true,
        },
    ];
    assert_eq!(
        *events.borrow(),
        expected
            .into_iter()
            .map(RetainedEvent::Input)
            .collect::<Vec<_>>()
    );
    assert!(session.host.key_held("z"));
    assert!(
        session
            .host
            .physical_key_held(PhysicalKey::Code(KeyCode::KeyW))
    );
}

fn pointer_sequence(session: &mut Session<'_>) {
    let device_id = DeviceId::dummy();
    for event in [
        WindowEvent::CursorEntered { device_id },
        WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(12.5, 80.25),
        },
        WindowEvent::MouseInput {
            device_id,
            state: ElementState::Pressed,
            button: MouseButton::Other(7),
        },
        WindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::LineDelta(1.5, -2.0),
            phase: TouchPhase::Moved,
        },
        WindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.5, -8.25)),
            phase: TouchPhase::Moved,
        },
    ] {
        session.event(&event, None);
    }
    session.device_event(&DeviceEvent::MouseMotion {
        delta: (3.25, -7.5),
    });
    session.event(&WindowEvent::CursorLeft { device_id }, None);
}

#[test]
fn focus_loss_releases_physical_keys_then_buttons_in_stable_order() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    for code in [KeyCode::KeyW, KeyCode::KeyA] {
        host.input(RetainedInput::Key {
            key: "same".into(),
            logical: crate::LogicalKey::Character("same".into()),
            physical: code.into(),
            pressed: true,
            repeat: false,
        });
    }
    for button in [MouseButton::Right, MouseButton::Left] {
        host.input(RetainedInput::Button {
            button,
            pressed: true,
        });
    }
    let released = host.input(RetainedInput::Focused(false));
    assert_eq!(
        released,
        vec![
            RetainedInput::Key {
                key: "same".into(),
                logical: crate::LogicalKey::Character("same".into()),
                physical: KeyCode::KeyA.into(),
                pressed: false,
                repeat: false
            },
            RetainedInput::Key {
                key: "same".into(),
                logical: crate::LogicalKey::Character("same".into()),
                physical: KeyCode::KeyW.into(),
                pressed: false,
                repeat: false
            },
            RetainedInput::Button {
                button: MouseButton::Left,
                pressed: false
            },
            RetainedInput::Button {
                button: MouseButton::Right,
                pressed: false
            },
            RetainedInput::Focused(false),
        ]
    );
    assert!(!host.button_held(MouseButton::Left));
    assert!(!host.key_held("same"));
    assert_eq!(
        host.input(RetainedInput::Focused(false)),
        vec![RetainedInput::Focused(false)]
    );
}

#[test]
fn cursor_requests_before_native_startup_return_typed_errors() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    assert!(matches!(
        host.set_cursor_grab(crate::CursorGrabMode::Locked),
        Err(crate::CursorError::Unavailable)
    ));
    assert!(matches!(
        host.set_cursor_visible(false),
        Err(crate::CursorError::Unavailable)
    ));
}

#[test]
fn invalid_fixed_interval_fails_before_native_startup() {
    for interval in [std::time::Duration::ZERO, std::time::Duration::MAX] {
        let options = crate::ViewerOptions {
            cadence: crate::PresentationCadence::Fixed(interval),
            ssr: None,
            render_clock: crate::RenderClock::default(),
            ..crate::tests::options()
        };
        assert!(matches!(
            RetainedHost::new(crate::tests::scene(false), options),
            Err(ViewerError::InvalidOptions(_))
        ));
    }
}

#[test]
fn releasing_one_physical_key_preserves_another_with_the_same_logical_key() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let logical = Key::Character("z".into());
    for code in [KeyCode::KeyA, KeyCode::KeyW] {
        host.input(super::super::input::key_input(
            &logical,
            code.into(),
            ElementState::Pressed,
            false,
        ));
    }
    host.input(super::super::input::key_input(
        &logical,
        KeyCode::KeyA.into(),
        ElementState::Released,
        false,
    ));
    assert!(host.key_held("z"));
    assert!(!host.physical_key_held(KeyCode::KeyA.into()));
}

#[cfg(target_os = "linux")]
pub(crate) fn native_window_pointer_delivery(window: std::sync::Arc<winit::window::Window>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut consumer = Consumer(events.clone());
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut session = Session::new(host, &mut consumer);
    session.window_ready(window);
    session
        .host
        .set_cursor_grab(crate::CursorGrabMode::None)
        .unwrap();
    session.host.set_cursor_visible(true).unwrap();
    pointer_sequence(&mut session);
    assert_eq!(events.borrow().len(), 7);
    assert_eq!(
        events.borrow()[1],
        RetainedEvent::Input(RetainedInput::PointerMoved([12.5, 80.25]))
    );
}
