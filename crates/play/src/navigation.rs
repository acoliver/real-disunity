//! Orbit navigation of the player window: the viewer's retained path draws a camera it is told
//! and leaves moving it to the application, so the window's pointer and keys turn an
//! [`OrbitCamera`] and the driver sends its pose on.

use worldrunner_render_scene::{CameraPose, Viewport};
use worldrunner_scene_viewer::{MouseButton, OrbitCamera, OrbitLimits, RetainedInput, WheelDelta};

/// Wheel pixels that count as one zoom step.
const PIXELS_PER_STEP: f64 = 40.0;

/// The orbit camera and the pointer state that moves it.
#[derive(Default)]
pub struct Navigation {
    orbit: Option<OrbitCamera>,
    dragging: bool,
    cursor: Option<[f64; 2]>,
    moved: bool,
}

impl Navigation {
    /// Starts navigating from `pose`, which the R and Home keys return to.
    pub fn start(&mut self, pose: CameraPose, limits: OrbitLimits) -> Result<(), String> {
        self.orbit = Some(OrbitCamera::new(pose, limits).map_err(|error| error.to_string())?);
        self.moved = false;
        Ok(())
    }

    /// Takes the camera pose when the input since the last call moved it.
    pub fn take_moved(&mut self) -> Option<CameraPose> {
        if !std::mem::take(&mut self.moved) {
            return None;
        }
        self.orbit.as_ref().map(OrbitCamera::pose)
    }

    /// Left-drag orbits, the wheel zooms and R or Home reset the camera. Before `start` the
    /// input moves nothing.
    pub fn accept(&mut self, input: &RetainedInput, size: [u32; 2]) {
        match input {
            RetainedInput::Button {
                button: MouseButton::Left,
                pressed,
            } => self.dragging = *pressed,
            RetainedInput::PointerMoved(position) => self.pointer(*position, size),
            RetainedInput::Wheel(delta) => {
                let steps = match delta {
                    WheelDelta::Lines([_, y]) => f64::from(*y),
                    WheelDelta::Pixels([_, y]) => y / PIXELS_PER_STEP,
                };
                if let Some(orbit) = &mut self.orbit {
                    orbit.zoom(steps);
                    self.moved = true;
                }
            }
            RetainedInput::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } if key == "r" || key == "home" => {
                if let Some(orbit) = &mut self.orbit {
                    orbit.reset();
                    self.moved = true;
                }
            }
            RetainedInput::CursorLeft => self.cursor = None,
            _ => {}
        }
    }

    fn pointer(&mut self, position: [f64; 2], size: [u32; 2]) {
        if self.dragging
            && let (Some(previous), Some(orbit)) = (self.cursor, &mut self.orbit)
        {
            let viewport = Viewport {
                width: size[0],
                height: size[1],
            };
            orbit.drag(
                [position[0] - previous[0], position[1] - previous[1]],
                viewport,
            );
            self.moved = true;
        }
        self.cursor = Some(position);
    }
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
