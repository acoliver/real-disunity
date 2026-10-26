use crate::{TimeSpan, World, WorldError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyCode(pub u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AxisId(pub u16);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AxisBinding {
    pub negative: Vec<KeyCode>,
    pub positive: Vec<KeyCode>,
    pub invert: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputAction {
    Key(KeyCode, bool),
    Axis(AxisId, f32),
    Focus(bool),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputEvent {
    pub timestamp: TimeSpan,
    pub action: InputAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputPhase {
    Fixed,
    Dynamic,
}
#[derive(Clone, Debug, PartialEq)]
pub struct InputView {
    pub held: BTreeSet<KeyCode>,
    pub down: BTreeSet<KeyCode>,
    pub up: BTreeSet<KeyCode>,
    pub axes: BTreeMap<AxisId, f32>,
    pub focused: bool,
    pub cutoff: TimeSpan,
    cursor: u64,
    device_axes: BTreeMap<AxisId, f32>,
    pub(crate) smoothed: BTreeMap<AxisId, f32>,
}
impl Default for InputView {
    fn default() -> Self {
        Self {
            held: BTreeSet::new(),
            down: BTreeSet::new(),
            up: BTreeSet::new(),
            axes: BTreeMap::new(),
            focused: true,
            cutoff: TimeSpan::ZERO,
            cursor: 0,
            device_axes: BTreeMap::new(),
            smoothed: BTreeMap::new(),
        }
    }
}
impl InputView {
    fn project(&mut self, events: &VecDeque<(u64, InputEvent)>, cutoff: TimeSpan) {
        self.down.clear();
        self.up.clear();
        for &(id, event) in events {
            if id <= self.cursor {
                continue;
            }
            if event.timestamp > cutoff {
                break;
            }
            match event.action {
                InputAction::Focus(focused) => {
                    self.focused = focused;
                    if !focused {
                        self.up.extend(std::mem::take(&mut self.held));
                        self.device_axes.clear();
                        self.smoothed.clear();
                    }
                }
                InputAction::Key(key, pressed) if self.focused => {
                    if pressed {
                        if self.held.insert(key) {
                            self.down.insert(key);
                        }
                    } else if self.held.remove(&key) {
                        self.up.insert(key);
                    }
                }
                InputAction::Axis(axis, value) if self.focused => {
                    self.device_axes.insert(axis, value);
                }
                InputAction::Key(_, _) | InputAction::Axis(_, _) => {}
            }
            self.cursor = id;
        }
        self.cutoff = cutoff;
    }
}
#[derive(Clone, Default)]
pub(crate) struct Input {
    pub(crate) bindings: BTreeMap<AxisId, AxisBinding>,
    pub(crate) named: BTreeMap<AxisId, crate::InputAxis>,
    queue: VecDeque<(u64, InputEvent)>,
    sequence: u64,
    last_timestamp: TimeSpan,
    published: Option<TimeSpan>,
    pub(crate) fixed: InputView,
    pub(crate) dynamic: InputView,
}
impl Input {
    pub(crate) fn project(&mut self, phase: InputPhase, cutoff: TimeSpan) {
        let previous = match phase {
            InputPhase::Fixed => self.fixed.cutoff,
            InputPhase::Dynamic => self.dynamic.cutoff,
        };
        match phase {
            InputPhase::Fixed => self.fixed.project(&self.queue, cutoff),
            InputPhase::Dynamic => self.dynamic.project(&self.queue, cutoff),
        }
        let view = match phase {
            InputPhase::Fixed => &mut self.fixed,
            InputPhase::Dynamic => &mut self.dynamic,
        };
        view.axes = self
            .bindings
            .iter()
            .map(|(&id, binding)| {
                let positive = binding.positive.iter().any(|key| view.held.contains(key));
                let negative = binding.negative.iter().any(|key| view.held.contains(key));
                let value = f32::from(i8::from(positive) - i8::from(negative));
                (id, if binding.invert { -value } else { value })
            })
            .collect();
        view.axes
            .extend(view.device_axes.iter().map(|(&id, &value)| (id, value)));
        crate::input_axes::project_named(
            view,
            &self.named,
            cutoff.checked_sub(previous).expect("ordered cutoffs"),
        );
        self.published = Some(self.published.map_or(cutoff, |old| old.max(cutoff)));
        let consumed = self.fixed.cursor.min(self.dynamic.cursor);
        while self.queue.front().is_some_and(|(id, _)| *id <= consumed) {
            self.queue.pop_front();
        }
    }
}
impl World {
    pub fn set_input_axes(
        &mut self,
        bindings: BTreeMap<AxisId, AxisBinding>,
    ) -> Result<(), WorldError> {
        if bindings.len() > 256
            || bindings
                .values()
                .any(|b| b.negative.len() + b.positive.len() > 32)
        {
            return Err(WorldError::InputCapacity);
        }
        self.input.bindings = bindings;
        self.input.named.clear();
        self.input.fixed.smoothed.clear();
        self.input.dynamic.smoothed.clear();
        Ok(())
    }
    pub fn input(&self, phase: InputPhase) -> &InputView {
        match phase {
            InputPhase::Fixed => &self.input.fixed,
            InputPhase::Dynamic => &self.input.dynamic,
        }
    }
    pub fn push_input(&mut self, events: &[InputEvent]) -> Result<(), WorldError> {
        if self.input.queue.len() + events.len() > 4096 {
            return Err(WorldError::InputCapacity);
        }
        let count = u64::try_from(events.len()).map_err(|_| WorldError::InputCapacity)?;
        self.input
            .sequence
            .checked_add(count)
            .ok_or(WorldError::InputCapacity)?;
        let mut timestamp = self.input.last_timestamp;
        for event in events {
            if event.timestamp < timestamp
                || self
                    .input
                    .published
                    .is_some_and(|cutoff| event.timestamp <= cutoff)
            {
                return Err(WorldError::InputOrder);
            }
            if let InputAction::Axis(_, value) = event.action
                && (!value.is_finite() || !(-1.0..=1.0).contains(&value))
            {
                return Err(WorldError::InvalidInput);
            }
            timestamp = event.timestamp;
        }
        for &event in events {
            self.input.sequence += 1;
            self.input.queue.push_back((self.input.sequence, event));
        }
        self.input.last_timestamp = timestamp;
        Ok(())
    }
}
