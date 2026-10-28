use super::{CursorError, CursorGrabMode, RetainedHost};

/// Event-time CPU controls. Scene edits and frame scheduling remain in `update`.
/// Cursor operations apply immediately and return platform errors without fallback.
pub struct RetainedEventHost<'a> {
    pub(super) host: &'a mut RetainedHost,
}

impl RetainedEventHost<'_> {
    pub fn request_exit(&mut self) {
        self.host.request_exit();
    }

    pub fn set_cursor_grab(&mut self, mode: CursorGrabMode) -> Result<(), CursorError> {
        self.host.set_cursor_grab(mode)
    }

    pub fn set_cursor_visible(&mut self, visible: bool) -> Result<(), CursorError> {
        self.host.set_cursor_visible(visible)
    }
}
