use super::{RetainedHost, RetainedInput, TextInput};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use winit::window::Window;
mod simulation;
pub(in crate::retained) mod text;

pub use winit::{
    event::MouseButton,
    keyboard::{Key as LogicalKey, KeyCode, NativeKeyCode, PhysicalKey},
    window::CursorGrabMode,
};

/// Wheel units are preserved, without applying a scrolling or camera policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    Lines([f32; 2]),
    Pixels([f64; 2]),
}

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("native cursor is unavailable before the window is created")]
    Unavailable,
    #[error("cursor grab {mode:?} rejected: {source}")]
    Grab {
        mode: CursorGrabMode,
        source: winit::error::ExternalError,
    },
}

/// Failure to request an operation on the retained session's native window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WindowControlError {
    #[error("native window is unavailable before it is created")]
    NoWindow,
    #[error("native window controls are unsupported in windowless sessions")]
    Unsupported,
}

pub(super) trait WindowControls: Send + Sync {
    fn focus_window(&self);
    fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), winit::error::ExternalError>;
    fn set_cursor_visible(&self, visible: bool);
    fn inner_size(&self) -> winit::dpi::PhysicalSize<u32>;
    fn set_text_input_area(&self, area: Option<[f64; 4]>);
    fn set_window_mode(&self, mode: crate::WindowMode);
    fn display(&self) -> Option<crate::DisplayInfo>;
}

impl WindowControls for Window {
    fn focus_window(&self) {
        Window::focus_window(self);
    }

    fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), winit::error::ExternalError> {
        Window::set_cursor_grab(self, mode)
    }

    fn set_cursor_visible(&self, visible: bool) {
        Window::set_cursor_visible(self, visible);
    }

    fn inner_size(&self) -> winit::dpi::PhysicalSize<u32> {
        Window::inner_size(self)
    }

    fn set_window_mode(&self, mode: crate::WindowMode) {
        let fullscreen = mode.fullscreen();
        let windowed = fullscreen.is_none();
        self.set_fullscreen(fullscreen);
        if windowed {
            self.set_maximized(mode == crate::WindowMode::Maximized);
        }
    }

    fn display(&self) -> Option<crate::DisplayInfo> {
        crate::window_size::display_info(self)
    }

    fn set_text_input_area(&self, area: Option<[f64; 4]>) {
        self.set_ime_allowed(area.is_some());
        if let Some([x, y, w, h]) = area {
            self.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(x, y),
                winit::dpi::PhysicalSize::new(w, h),
            );
        }
    }
}

#[derive(Default)]
pub(super) enum WindowControlTarget {
    #[default]
    Pending,
    Windowless,
    Native(Arc<dyn WindowControls>),
}

impl WindowControlTarget {
    pub(super) fn native(&self) -> Result<&dyn WindowControls, WindowControlError> {
        match self {
            Self::Pending => Err(WindowControlError::NoWindow),
            Self::Windowless => Err(WindowControlError::Unsupported),
            Self::Native(window) => Ok(window.as_ref()),
        }
    }

    fn cursor(&self) -> Result<&dyn WindowControls, CursorError> {
        self.native().map_err(|_| CursorError::Unavailable)
    }
}

#[derive(Default)]
pub(super) struct InputState {
    pub text: text::TextState,
    keys: BTreeMap<PhysicalKey, (String, LogicalKey)>,
    buttons: BTreeSet<MouseButton>,
    pub window: WindowControlTarget,
    pub surface_extent: Option<[u32; 2]>,
}

impl InputState {
    pub fn accept(&mut self, input: &RetainedInput) -> Vec<RetainedInput> {
        match input {
            RetainedInput::Key {
                key,
                logical,
                physical,
                pressed,
                ..
            } => {
                if *pressed {
                    self.keys.insert(*physical, (key.clone(), logical.clone()));
                } else {
                    self.keys.remove(physical);
                }
            }
            RetainedInput::Button { button, pressed } => {
                if *pressed {
                    self.buttons.insert(*button);
                } else {
                    self.buttons.remove(button);
                }
            }
            RetainedInput::Text(TextInput::Preedit { text, .. }) => {
                self.text.composing = !text.is_empty();
            }
            RetainedInput::Text(TextInput::Commit(_) | TextInput::Cancel | TextInput::Disabled) => {
                self.text.composing = false;
            }
            RetainedInput::Focused(false) => return self.release(),
            _ => {}
        }
        Vec::new()
    }

    fn release(&mut self) -> Vec<RetainedInput> {
        let mut events: Vec<_> = std::mem::take(&mut self.keys)
            .into_iter()
            .map(|(physical, (key, logical))| RetainedInput::Key {
                key,
                logical,
                physical,
                pressed: false,
                repeat: false,
            })
            .collect();
        if !events.is_empty() || !self.buttons.is_empty() || self.text.area.is_some() {
            events.insert(0, RetainedInput::PointerCancelled);
        }
        events.extend(std::mem::take(&mut self.buttons).into_iter().map(|button| {
            RetainedInput::Button {
                button,
                pressed: false,
            }
        }));
        if self.text.area.is_some() {
            events.insert(0, RetainedInput::Text(TextInput::Cancel));
        }
        self.text.composing = false;
        events
    }
}

impl RetainedHost {
    pub fn key_held(&self, key: &str) -> bool {
        self.input_state.keys.values().any(|(held, _)| held == key)
    }

    pub fn physical_key_held(&self, key: PhysicalKey) -> bool {
        self.input_state.keys.contains_key(&key)
    }

    pub fn button_held(&self, button: MouseButton) -> bool {
        self.input_state.buttons.contains(&button)
    }

    /// Requests native focus immediately; the window system can decline it.
    /// Returns `NoWindow` before native startup and `Unsupported` when windowless.
    /// Actual focus changes arrive only through `RetainedInput::Focused` events.
    pub fn focus_window(&mut self) -> Result<(), WindowControlError> {
        self.input_state.window.native()?.focus_window();
        Ok(())
    }

    /// Applies exactly the requested mode. Platform rejection is returned without fallback.
    pub fn set_cursor_grab(&mut self, mode: CursorGrabMode) -> Result<(), CursorError> {
        let window = self.input_state.window.cursor()?;
        grab(mode, |mode| window.set_cursor_grab(mode))
    }

    pub fn set_cursor_visible(&mut self, visible: bool) -> Result<(), CursorError> {
        let window = self.input_state.window.cursor()?;
        window.set_cursor_visible(visible);
        Ok(())
    }

    /// Requests a window placement immediately; the window system can constrain it.
    /// The resulting surface arrives as `RetainedInput::Resized`, through the same
    /// reconfiguration and presentation budget as a user resize.
    pub fn set_window_mode(&mut self, mode: crate::WindowMode) -> Result<(), WindowControlError> {
        self.input_state.window.native()?.set_window_mode(mode);
        Ok(())
    }

    /// The monitor the native window currently occupies, or `None` when the platform
    /// cannot identify it. Valid from the first `RetainedInput::DisplayChanged`.
    pub fn display(&self) -> Result<Option<crate::DisplayInfo>, WindowControlError> {
        Ok(self.input_state.window.native()?.display())
    }
}

fn grab(
    mode: CursorGrabMode,
    apply: impl FnOnce(CursorGrabMode) -> Result<(), winit::error::ExternalError>,
) -> Result<(), CursorError> {
    apply(mode).map_err(|source| CursorError::Grab { mode, source })
}

pub(super) fn window_input(event: &winit::event::WindowEvent) -> Option<RetainedInput> {
    use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
    Some(match event {
        WindowEvent::Ime(event) => text::ime_input(event),
        WindowEvent::Resized(size) => RetainedInput::Resized([size.width, size.height]),
        WindowEvent::Focused(value) => RetainedInput::Focused(*value),
        WindowEvent::Occluded(value) => RetainedInput::Occluded(*value),
        WindowEvent::CloseRequested => RetainedInput::CloseRequested,
        WindowEvent::CursorMoved { position, .. } => {
            RetainedInput::PointerMoved([position.x, position.y])
        }
        WindowEvent::CursorEntered { .. } => RetainedInput::CursorEntered,
        WindowEvent::CursorLeft { .. } => RetainedInput::CursorLeft,
        WindowEvent::MouseInput { state, button, .. } => RetainedInput::Button {
            button: *button,
            pressed: *state == ElementState::Pressed,
        },
        WindowEvent::MouseWheel { delta, .. } => RetainedInput::Wheel(match delta {
            MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines([*x, *y]),
            MouseScrollDelta::PixelDelta(position) => WheelDelta::Pixels([position.x, position.y]),
        }),
        WindowEvent::KeyboardInput { event, .. } => key_input(
            &event.logical_key,
            event.physical_key,
            event.state,
            event.repeat,
        ),
        _ => return None,
    })
}

pub(super) fn key_input(
    logical: &winit::keyboard::Key,
    physical: PhysicalKey,
    state: winit::event::ElementState,
    repeat: bool,
) -> RetainedInput {
    use winit::keyboard::Key;
    let key = match logical {
        Key::Character(value) => value.to_lowercase(),
        Key::Named(value) => format!("{value:?}").to_lowercase(),
        _ => match physical {
            PhysicalKey::Code(code) => format!("{code:?}").to_lowercase(),
            PhysicalKey::Unidentified(code) => format!("{code:?}").to_lowercase(),
        },
    };
    RetainedInput::Key {
        key,
        logical: logical.clone(),
        physical,
        pressed: state == winit::event::ElementState::Pressed,
        repeat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_grab_preserves_mode_and_platform_error_without_fallback() {
        let error = grab(CursorGrabMode::Locked, |mode| {
            assert_eq!(mode, CursorGrabMode::Locked);
            Err(winit::error::ExternalError::Ignored)
        })
        .unwrap_err();
        assert!(matches!(
            error,
            CursorError::Grab {
                mode: CursorGrabMode::Locked,
                source: winit::error::ExternalError::Ignored
            }
        ));
    }
}
