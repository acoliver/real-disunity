use super::{UiAction, UiControlKind, UiKey, UiResponse, UiRouter};
use crate::ui::{RectLayout, UiAxis, UiLayout};
use crate::{ObjectHandle, WorldError};
use glam::DVec2;

/// Validated range and current value. `None` means continuous pointer movement and a keyboard
/// increment of one percent of the range. Both endpoints remain reachable with uneven steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSlider {
    min: f64,
    max: f64,
    step: Option<f64>,
    value: f64,
    axis: UiAxis,
}
impl UiSlider {
    pub fn new(
        min: f64,
        max: f64,
        step: Option<f64>,
        value: f64,
        axis: UiAxis,
    ) -> Result<Self, WorldError> {
        let span = max - min;
        if !min.is_finite() || !max.is_finite() || !span.is_finite() || min >= max {
            return Err(WorldError::InvalidInput);
        }
        if !value.is_finite()
            || step.is_some_and(|s| !s.is_finite() || s <= 0. || !(span / s).is_finite())
        {
            return Err(WorldError::InvalidInput);
        }
        let mut slider = Self {
            min,
            max,
            step,
            value,
            axis,
        };
        slider.value = slider.snap(value);
        Ok(slider)
    }
    pub fn value(self) -> f64 {
        self.value
    }
    /// Fraction along the local axis, for caller-authored fill extents or knob placement.
    pub fn normalized_value(self) -> f64 {
        (self.value - self.min) / (self.max - self.min)
    }
    pub fn axis(self) -> UiAxis {
        self.axis
    }
    fn snap(self, value: f64) -> f64 {
        if value <= self.min {
            return self.min;
        }
        if value >= self.max {
            return self.max;
        }
        match self.step {
            Some(step) => {
                (self.min + ((value - self.min) / step).round() * step).clamp(self.min, self.max)
            }
            None => value,
        }
    }
    fn key_value(self, key: UiKey) -> Option<f64> {
        let increment = self.step.unwrap_or((self.max - self.min) / 100.);
        match key {
            UiKey::Left | UiKey::Down => Some(self.value - increment),
            UiKey::Right | UiKey::Up => Some(self.value + increment),
            UiKey::Home => Some(self.min),
            UiKey::End => Some(self.max),
            _ => None,
        }
    }
}
impl UiRouter {
    pub fn slider(&self, object: ObjectHandle) -> Option<UiSlider> {
        match self.controls.get(&object)?.kind {
            UiControlKind::Slider(slider) => Some(slider),
            _ => None,
        }
    }
    pub(super) fn slider_key(&mut self, key: UiKey) -> Option<UiAction> {
        let object = self.focus?;
        let value = self.slider(object)?.key_value(key)?;
        self.set_slider(object, value)
    }
    pub(super) fn drag_slider(&mut self, layout: &UiLayout, response: &mut UiResponse) {
        let Some((object, slider)) = self.capture.and_then(|o| self.slider(o).map(|s| (o, s)))
        else {
            return;
        };
        let Some(position) = self.position else {
            return;
        };
        let Some(rect) = layout.rect(object) else {
            return;
        };
        let Some(fraction) = fraction(rect, layout, position, slider.axis) else {
            return;
        };
        let value = if fraction <= 0. {
            slider.min
        } else if fraction >= 1. {
            slider.max
        } else {
            slider.min + fraction * (slider.max - slider.min)
        };
        response.actions.extend(self.set_slider(object, value));
    }
    fn set_slider(&mut self, object: ObjectHandle, value: f64) -> Option<UiAction> {
        let UiControlKind::Slider(slider) = &mut self.controls.get_mut(&object)?.kind else {
            return None;
        };
        let value = slider.snap(value);
        // Event edges require exact numeric equality, including signed zero.
        if value.partial_cmp(&slider.value) == Some(std::cmp::Ordering::Equal) {
            return None;
        }
        slider.value = value;
        Some(UiAction::ValueChanged {
            control: object,
            value,
        })
    }
}
fn fraction(rect: &RectLayout, layout: &UiLayout, position: [f64; 2], axis: UiAxis) -> Option<f64> {
    let point = DVec2::new(position[0], f64::from(layout.viewport.y) - position[1]);
    let local = super::targets::coordinates(rect, point)?;
    Some(match axis {
        UiAxis::Horizontal => local.x,
        UiAxis::Vertical => local.y,
    })
}
