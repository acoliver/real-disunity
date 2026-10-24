use super::{
    axes::Axis,
    event::Event,
    projection::{Phase, Projection},
    state::State,
};
use crate::ratio::Ratio;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

pub(crate) const MAX_INPUT_EVENTS: usize = 4096;

#[derive(Clone, Default)]
pub(super) struct Service {
    queue: VecDeque<Event>,
    head: State,
    last_event_id: u64,
    last_timestamp: Ratio,
    pub dynamic: Projection,
    fixed: Projection,
}
impl Service {
    pub fn at(timestamp: Ratio, frame: u64, fixed_frame: u64) -> Self {
        Self {
            dynamic: Projection::at(timestamp, frame),
            fixed: Projection::at(timestamp, fixed_frame),
            ..Self::default()
        }
    }
    pub fn enqueue(&mut self, events: Vec<Event>) -> Result<Value, String> {
        if events.is_empty()
            || events.len() > MAX_INPUT_EVENTS
            || self.queue.len() + events.len() > MAX_INPUT_EVENTS
        {
            return Err(
                "input event batch/queue requires 1..4096 records within queue capacity".into(),
            );
        }
        let mut accepted = Vec::with_capacity(events.len());
        for event in events {
            self.validate_order(&event)?;
            self.head.apply(&event)?;
            self.last_event_id = event.event_id;
            self.last_timestamp = event.timestamp;
            accepted.push(event.event_id);
            self.queue.push_back(event);
        }
        Ok(json!({"accepted_event_ids":accepted,"queued_events":self.queue.len()}))
    }
    fn validate_order(&self, event: &Event) -> Result<(), String> {
        if event.event_id <= self.last_event_id || event.timestamp < self.last_timestamp {
            return Err("input event IDs must increase and timestamps must not decrease".into());
        }
        for projection in [&self.dynamic, &self.fixed] {
            if projection.projection_id > 0 && event.timestamp <= projection.timestamp {
                return Err("input event timestamp is at or before a published cutoff".into());
            }
        }
        Ok(())
    }
    pub fn get(&self, phase: Phase) -> Value {
        match phase {
            Phase::Dynamic => json!(self.dynamic),
            Phase::Fixed => json!(self.fixed),
        }
    }
    pub fn status(&self) -> Value {
        json!({"mode":"timestamped","queued_events":self.queue.len(),
            "last_event_id":self.last_event_id,"last_timestamp":self.last_timestamp,
            "dynamic_projection":self.dynamic.projection_id,"fixed_projection":self.fixed.projection_id})
    }
    pub fn advance(
        &mut self,
        phase: Phase,
        timestamp: Ratio,
        id: u64,
        definitions: &BTreeMap<String, Axis>,
    ) -> Result<(), String> {
        let projection = match phase {
            Phase::Dynamic => &mut self.dynamic,
            Phase::Fixed => &mut self.fixed,
        };
        projection.advance(&self.queue, timestamp, id, definitions)?;
        let consumed = self.dynamic.last_event_id.min(self.fixed.last_event_id);
        while self.queue.front().is_some_and(|e| e.event_id <= consumed) {
            self.queue.pop_front();
        }
        Ok(())
    }
}
