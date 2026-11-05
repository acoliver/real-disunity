#![cfg(feature = "scene-viewer")]

use worldrunner::scene::viewer::{RetainedEventHost, RetainedHost, WindowControlError};

#[test]
fn facade_retained_focus_controls_are_typed_and_public() {
    let _: fn(&mut RetainedHost) -> Result<(), WindowControlError> = RetainedHost::focus_window;
    let _: fn(&mut RetainedEventHost<'_>) -> Result<(), WindowControlError> =
        |host| host.focus_window();
    for error in [
        WindowControlError::NoWindow,
        WindowControlError::Unsupported,
    ] {
        let error: &dyn std::error::Error = &error;
        assert!(!error.to_string().is_empty());
    }
}
