use super::{axes::Axis, event::Event, state::State};
use crate::ratio::Ratio;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    #[default]
    Dynamic,
    Fixed,
}

#[derive(Clone, Serialize)]
pub(crate) struct Projection {
    pub keys: BTreeSet<String>,
    pub down: BTreeSet<String>,
    pub up: BTreeSet<String>,
    pub axes: BTreeMap<String, f32>,
    pub focused: bool,
    pub projection_id: u64,
    pub timestamp: Ratio,
    pub last_event_id: u64,
    #[serde(skip)]
    state: State,
}
impl Default for Projection {
    fn default() -> Self {
        Self {
            keys: BTreeSet::new(),
            down: BTreeSet::new(),
            up: BTreeSet::new(),
            axes: BTreeMap::new(),
            focused: true,
            projection_id: 0,
            timestamp: Ratio::ZERO,
            last_event_id: 0,
            state: State::default(),
        }
    }
}
impl Projection {
    pub fn at(timestamp: Ratio, projection_id: u64) -> Self {
        Self {
            timestamp,
            projection_id,
            ..Self::default()
        }
    }
    pub fn advance(
        &mut self,
        queue: &VecDeque<Event>,
        timestamp: Ratio,
        id: u64,
        definitions: &BTreeMap<String, Axis>,
    ) -> Result<(), String> {
        if timestamp < self.timestamp || id <= self.projection_id {
            return Err("input projection cannot move backwards".into());
        }
        self.down.clear();
        self.up.clear();
        for event in queue {
            if event.event_id <= self.last_event_id || event.timestamp > timestamp {
                continue;
            }
            let before = self.state.keys();
            self.state.apply(event)?;
            let after = self.state.keys();
            self.down.extend(after.difference(&before).cloned());
            self.up.extend(before.difference(&after).cloned());
            self.last_event_id = event.event_id;
        }
        self.keys = self.state.keys();
        self.axes = self.state.axes(definitions);
        self.focused = self.state.focused;
        self.timestamp = timestamp;
        self.projection_id = id;
        Ok(())
    }
}
