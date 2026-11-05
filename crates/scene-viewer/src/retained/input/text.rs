use super::{RetainedHost, RetainedInput};
use crate::{RetainedEventHost, ViewerError};
pub use worldrunner_world_simulation::TextInput;

pub(in crate::retained) struct TextState {
    pub area: Option<[f64; 4]>,
    pub composing: bool,
    pub scale: f64,
}
impl Default for TextState {
    fn default() -> Self {
        Self {
            area: None,
            composing: false,
            scale: 1.,
        }
    }
}
impl RetainedHost {
    pub fn scale_factor(&self) -> f64 {
        self.input_state.text.scale
    }
    pub fn text_input_area(&self) -> Option<[f64; 4]> {
        self.input_state.text.area
    }
    /// Caller supplies the focused text field's physical top-left cursor rectangle;
    /// None disables IME. Works before window creation and in windowless sessions.
    pub fn set_text_input_area(&mut self, area: Option<[f64; 4]>) -> Result<(), ViewerError> {
        if area.is_some_and(|a| a.into_iter().any(|v| !v.is_finite()) || a[2] < 0. || a[3] < 0.) {
            return Err(ViewerError::InvalidOptions("invalid IME cursor area"));
        }
        if self.input_state.text.area != area {
            if self.input_state.text.area.is_some() != area.is_some() {
                self.input_state.text.composing = false;
            }
            self.input_state.text.area = area;
            self.apply_text_input_area();
        }
        Ok(())
    }
    pub(in crate::retained) fn apply_text_input_area(&self) {
        if let super::WindowControlTarget::Native(window) = &self.input_state.window {
            window.set_text_input_area(self.text_input_area());
        }
    }
}
impl RetainedEventHost<'_> {
    pub fn set_text_input_area(&mut self, area: Option<[f64; 4]>) -> Result<(), ViewerError> {
        self.host.set_text_input_area(area)
    }
    pub fn scale_factor(&self) -> f64 {
        self.host.scale_factor()
    }
    pub fn size(&self) -> [u32; 2] {
        self.host.size()
    }
}
pub(in crate::retained) fn keyboard_text(
    text: Option<&str>,
    pressed: bool,
) -> Option<RetainedInput> {
    text.filter(|s| pressed && !s.is_empty() && !s.chars().all(char::is_control))
        .map(|s| RetainedInput::Text(TextInput::Commit(s.into())))
}
pub(super) fn ime_input(event: &winit::event::Ime) -> RetainedInput {
    use winit::event::Ime;
    RetainedInput::Text(match event {
        Ime::Enabled => TextInput::Enabled,
        Ime::Preedit(text, cursor) => TextInput::Preedit {
            text: text.clone(),
            cursor: *cursor,
        },
        Ime::Commit(text) => TextInput::Commit(text.clone()),
        Ime::Disabled => TextInput::Disabled,
    })
}
