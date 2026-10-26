use super::{RetainedHost, RetainedInput};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use winit::window::Window;

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

#[derive(Default)]
pub(super) struct InputState {
    keys: BTreeMap<PhysicalKey, (String, LogicalKey)>,
    buttons: BTreeSet<MouseButton>,
    pub window: Option<Arc<Window>>,
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
        events.extend(std::mem::take(&mut self.buttons).into_iter().map(|button| {
            RetainedInput::Button {
                button,
                pressed: false,
            }
        }));
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

    /// Applies exactly the requested mode. Platform rejection is returned without fallback.
    pub fn set_cursor_grab(&mut self, mode: CursorGrabMode) -> Result<(), CursorError> {
        let window = self
            .input_state
            .window
            .as_ref()
            .ok_or(CursorError::Unavailable)?;
        grab(mode, |mode| window.set_cursor_grab(mode))
    }

    pub fn set_cursor_visible(&mut self, visible: bool) -> Result<(), CursorError> {
        let window = self
            .input_state
            .window
            .as_ref()
            .ok_or(CursorError::Unavailable)?;
        window.set_cursor_visible(visible);
        Ok(())
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
