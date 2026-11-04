use std::collections::BTreeSet;

/// Native mouse button identity, independent of a window backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Wheel units are retained rather than choosing a game-specific scroll speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerWheel {
    Lines([f64; 2]),
    Pixels([f64; 2]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    /// Physical host pixels, with origin at the top left. Do not divide by display scale.
    Moved([f64; 2]),
    Button {
        button: PointerButton,
        pressed: bool,
    },
    Wheel(PointerWheel),
    /// Ends capture without activation, before any synthetic button releases.
    Cancel,
}
impl PointerEvent {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Moved(value)
            | Self::Wheel(PointerWheel::Lines(value) | PointerWheel::Pixels(value)) => {
                value.into_iter().all(f64::is_finite)
            }
            _ => true,
        }
    }
}

/// A phase-specific frame projection. Edges and wheel deltas clear on the next projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointerState {
    pub position: Option<[f64; 2]>,
    pub held: BTreeSet<PointerButton>,
    pub down: BTreeSet<PointerButton>,
    pub up: BTreeSet<PointerButton>,
    pub wheel_lines: [f64; 2],
    pub wheel_pixels: [f64; 2],
    /// Ordered observations for routing gestures even when press and release share a frame.
    pub events: Vec<PointerEvent>,
}
impl PointerState {
    pub(super) fn begin(&mut self) {
        self.down.clear();
        self.up.clear();
        self.wheel_lines = [0.; 2];
        self.wheel_pixels = [0.; 2];
        self.events.clear();
    }

    pub(super) fn accept(&mut self, event: PointerEvent) {
        self.events.push(event);
        match event {
            PointerEvent::Moved(position) => self.position = Some(position),
            PointerEvent::Button { button, pressed } => {
                if pressed {
                    if self.held.insert(button) {
                        self.down.insert(button);
                    }
                } else if self.held.remove(&button) {
                    self.up.insert(button);
                }
            }
            PointerEvent::Wheel(PointerWheel::Lines(delta)) => {
                for (sum, value) in self.wheel_lines.iter_mut().zip(delta) {
                    *sum += value;
                }
            }
            PointerEvent::Wheel(PointerWheel::Pixels(delta)) => {
                for (sum, value) in self.wheel_pixels.iter_mut().zip(delta) {
                    *sum += value;
                }
            }
            PointerEvent::Cancel => {
                self.up.extend(std::mem::take(&mut self.held));
                self.position = None;
            }
        }
    }
}
