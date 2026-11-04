use super::*;
use worldrunner_world_simulation::ui::{ClipboardCommand, ClipboardError};

#[test]
fn clipboard_is_shared_by_update_and_event_hosts_without_a_window() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    host.clipboard().write_text("AB").unwrap();
    let mut events = RetainedEventHost { host: &mut host };
    assert_eq!(events.clipboard().read_text().unwrap(), "AB");
    events.clipboard().write_text("CD").unwrap();
    assert_eq!(host.clipboard().read_text().unwrap(), "CD");
}
#[test]
fn failed_native_clipboard_remains_a_typed_error_without_memory_fallback() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    host.clipboard = clipboard::HostClipboard::Native(Err(ClipboardError::Unavailable));
    assert_eq!(
        host.clipboard().read_text(),
        Err(ClipboardError::Unavailable)
    );
    assert_eq!(
        host.clipboard().write_text("AB"),
        Err(ClipboardError::Unavailable)
    );
}
#[test]
fn host_shortcuts_ignore_releases_repeats_and_wrong_modifiers() {
    use winit::{event::ElementState, keyboard::Key};
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let primary = if cfg!(target_os = "macos") {
        "super"
    } else {
        "control"
    };
    let held = |name: &str, code| RetainedInput::Key {
        key: name.into(),
        logical: Key::Character(name.into()),
        physical: PhysicalKey::Code(code),
        pressed: true,
        repeat: false,
    };
    let mut key = input::key_input(
        &Key::Character("v".into()),
        PhysicalKey::Code(KeyCode::KeyV),
        ElementState::Pressed,
        false,
    );
    assert_eq!(host.clipboard_command(&key), None);
    host.input(held(primary, KeyCode::SuperLeft));
    assert_eq!(host.clipboard_command(&key), Some(ClipboardCommand::Paste));
    assert!(host.command_modifier_held());
    if let RetainedInput::Key { repeat, .. } = &mut key {
        *repeat = true;
    }
    assert_eq!(host.clipboard_command(&key), None);
    if let RetainedInput::Key { pressed, .. } = &mut key {
        *pressed = false;
    }
    assert_eq!(host.clipboard_command(&key), None);
    host.input(RetainedInput::Focused(false));
    assert!(!host.command_modifier_held());
}
