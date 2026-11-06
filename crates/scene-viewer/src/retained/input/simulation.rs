use super::{MouseButton, RetainedInput, WheelDelta};
use worldrunner_world_simulation::{InputAction, PointerButton, PointerEvent, PointerWheel};

impl RetainedInput {
    /// Native simulation observation. Host pixels are already physical, including on scaled
    /// displays. Key/action bindings remain caller policy. Queue with the host event timestamp.
    pub fn simulation_action(&self) -> Option<InputAction> {
        let pointer = match self {
            Self::PointerMoved(position) => PointerEvent::Moved(*position),
            Self::PointerCancelled => PointerEvent::Cancel,
            Self::Button { button, pressed } => PointerEvent::Button {
                button: pointer_button(*button),
                pressed: *pressed,
            },
            Self::Wheel(WheelDelta::Lines(delta)) => {
                PointerEvent::Wheel(PointerWheel::Lines(delta.map(f64::from)))
            }
            Self::Wheel(WheelDelta::Pixels(delta)) => {
                PointerEvent::Wheel(PointerWheel::Pixels(*delta))
            }
            Self::Focused(value) => return Some(InputAction::Focus(*value)),
            Self::Text(text) => return Some(InputAction::Text(text.clone())),
            _ => return None,
        };
        Some(InputAction::Pointer(pointer))
    }
}
fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        MouseButton::Other(code) => PointerButton::Other(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retained::input::{InputState, window_input};
    #[test]
    fn physical_pixels_and_wheel_units_reach_simulation_unchanged() {
        let event = window_input(&winit::event::WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
                2., -3.,
            )),
            phase: winit::event::TouchPhase::Moved,
        })
        .unwrap();
        assert_eq!(
            event.simulation_action(),
            Some(InputAction::Pointer(PointerEvent::Wheel(
                PointerWheel::Pixels([2., -3.])
            )))
        );
        assert_eq!(
            RetainedInput::PointerMoved([800., 600.]).simulation_action(),
            Some(InputAction::Pointer(PointerEvent::Moved([800., 600.])))
        );
        assert_eq!(
            RetainedInput::Wheel(WheelDelta::Lines([1., -2.])).simulation_action(),
            Some(InputAction::Pointer(PointerEvent::Wheel(
                PointerWheel::Lines([1., -2.])
            )))
        );
    }
    #[test]
    fn focus_loss_cancels_before_releasing_buttons() {
        let mut state = InputState::default();
        state.accept(&RetainedInput::Button {
            button: MouseButton::Left,
            pressed: true,
        });
        let events = state.accept(&RetainedInput::Focused(false));
        assert_eq!(
            events
                .iter()
                .filter_map(RetainedInput::simulation_action)
                .collect::<Vec<_>>(),
            vec![
                InputAction::Pointer(PointerEvent::Cancel),
                InputAction::Pointer(PointerEvent::Button {
                    button: PointerButton::Left,
                    pressed: false
                })
            ]
        );
        assert!(state.accept(&RetainedInput::Focused(false)).is_empty());
    }
}
