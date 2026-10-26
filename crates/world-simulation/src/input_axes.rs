use crate::{AxisBinding, AxisId, InputPhase, World, WorldError};

/// One project-authored keyboard axis. Alternate keys share the binding lists.
#[derive(Clone, Debug, PartialEq)]
pub struct InputAxis {
    pub name: String,
    pub binding: AxisBinding,
    pub gravity: f32,
    pub sensitivity: f32,
    pub dead: f32,
    pub snap: bool,
}
impl InputAxis {
    fn validate(&self) -> Result<(), WorldError> {
        if self.name.is_empty()
            || self.name.len() > 1024
            || self.binding.negative.len() + self.binding.positive.len() > 32
            || !self.gravity.is_finite()
            || self.gravity < 0.0
            || !self.sensitivity.is_finite()
            || self.sensitivity < 0.0
            || !self.dead.is_finite()
            || !(0.0..=1.0).contains(&self.dead)
        {
            return Err(WorldError::InvalidInput);
        }
        Ok(())
    }
    pub(crate) fn raw(&self, value: f32) -> f32 {
        let value = if value.abs() < self.dead { 0.0 } else { value };
        if self.binding.invert { -value } else { value }
    }
    pub(crate) fn smooth(&self, previous: f32, target: f32, seconds: f32) -> f32 {
        let previous = if self.snap && previous * target < 0.0 {
            0.0
        } else {
            previous
        };
        let rate = if target == 0.0 {
            self.gravity
        } else {
            self.sensitivity
        };
        previous + (target - previous).clamp(-rate * seconds, rate * seconds)
    }
}
impl World {
    /// Registers or replaces an ID atomically. Names may repeat across IDs.
    pub fn register_input_axis(&mut self, id: AxisId, axis: InputAxis) -> Result<(), WorldError> {
        axis.validate()?;
        if !self.input.bindings.contains_key(&id) && self.input.bindings.len() >= 256 {
            return Err(WorldError::InputCapacity);
        }
        let mut binding = axis.binding.clone();
        binding.invert = false;
        self.input.bindings.insert(id, binding);
        self.input.named.insert(id, axis);
        self.input.fixed.smoothed.remove(&id);
        self.input.dynamic.smoothed.remove(&id);
        self.input.fixed.axes.remove(&id);
        self.input.dynamic.axes.remove(&id);
        Ok(())
    }
    pub fn input_axis_id(&self, name: &str) -> Result<AxisId, WorldError> {
        self.input
            .named
            .iter()
            .find_map(|(&id, axis)| (axis.name == name).then_some(id))
            .ok_or(WorldError::InvalidInput)
    }
    /// Duplicate names select the greatest magnitude; lowest ID breaks equal ties.
    pub fn input_axis(&self, phase: InputPhase, name: &str, raw: bool) -> Result<f32, WorldError> {
        self.input_axis_id(name)?;
        let view = self.input(phase);
        let values = if raw { &view.axes } else { &view.smoothed };
        Ok(self
            .input
            .named
            .iter()
            .filter(|(_, axis)| axis.name == name)
            .map(|(id, _)| values.get(id).copied().unwrap_or(0.0))
            .fold(0.0_f32, |best, value| {
                if value.abs() > best.abs() {
                    value
                } else {
                    best
                }
            }))
    }
}

pub(crate) fn project_named(
    view: &mut crate::InputView,
    axes: &std::collections::BTreeMap<AxisId, InputAxis>,
    elapsed: crate::TimeSpan,
) {
    // Input projection intervals are bounded by the clock's finite frame duration.
    let seconds = std::time::Duration::from_secs_f64(elapsed.seconds()).as_secs_f32();
    for (&id, axis) in axes {
        let raw = axis.raw(view.axes.get(&id).copied().unwrap_or(0.0));
        let previous = view.smoothed.get(&id).copied().unwrap_or(0.0);
        let value = if view.focused {
            axis.smooth(previous, raw, seconds)
        } else {
            0.0
        };
        view.axes.insert(id, raw);
        view.smoothed.insert(id, value);
    }
}
