use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) mod axes;
pub(crate) mod event;
pub(crate) mod projection;
mod service;
mod state;
pub(crate) use service::MAX_INPUT_EVENTS;
#[derive(Clone, Serialize)]
pub(crate) struct Input {
    pub keys: BTreeSet<String>,
    pub down: BTreeSet<String>,
    pub up: BTreeSet<String>,
    pub axes: BTreeMap<String, f32>,
    pub focused: bool,
    #[serde(skip)]
    definitions: BTreeMap<String, axes::Axis>,
    #[serde(skip)]
    service: Option<service::Service>,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            keys: BTreeSet::new(),
            down: BTreeSet::new(),
            up: BTreeSet::new(),
            axes: BTreeMap::new(),
            focused: true,
            definitions: BTreeMap::new(),
            service: None,
        }
    }
}
impl Input {
    pub fn configure(
        &mut self,
        definitions: BTreeMap<String, axes::Axis>,
    ) -> Result<Value, String> {
        axes::validate(&definitions)?;
        self.definitions = definitions;
        if self.service.is_none() {
            self.axes = self.project();
        }
        Ok(self.snapshot(projection::Phase::Dynamic))
    }
    fn project(&self) -> BTreeMap<String, f32> {
        self.definitions
            .iter()
            .map(|(name, axis)| (name.clone(), axis.raw(&self.keys)))
            .collect()
    }
    pub fn focus(&mut self, focused: bool) -> Result<Value, String> {
        self.require_legacy()?;
        self.focused = focused;
        if !focused {
            return self.set(BTreeSet::new(), BTreeMap::new());
        }
        Ok(json!(self))
    }
    pub fn set(
        &mut self,
        mut keys: BTreeSet<String>,
        mut axes: BTreeMap<String, f32>,
    ) -> Result<Value, String> {
        self.require_legacy()?;
        if axes.values().any(|v| !v.is_finite())
            || axes.len() > 256
            || keys.len() > 256
            || axes.keys().chain(&keys).any(|k| !axes::valid_name(k))
        {
            return Err("invalid input keys/axes or input budget exceeded".into());
        }
        if !self.focused {
            keys.clear();
            axes.clear();
        }
        self.down = keys.difference(&self.keys).cloned().collect();
        self.up = self.keys.difference(&keys).cloned().collect();
        self.keys = keys;
        self.axes = self.project();
        self.axes.extend(axes);
        Ok(json!(self))
    }
}
impl Input {
    fn require_legacy(&self) -> Result<(), String> {
        if self.service.is_some() {
            return Err("timestamped input requires input.events".into());
        }
        Ok(())
    }
    pub fn snapshot(&self, phase: projection::Phase) -> Value {
        self.service
            .as_ref()
            .map_or_else(|| json!(self), |s| s.get(phase))
    }
    pub fn status(&self) -> Value {
        self.service.as_ref().map_or_else(
            || {
                json!({"mode":"legacy","queued_events":0,
            "last_event_id":0,"last_timestamp":crate::ratio::Ratio::ZERO,
            "dynamic_projection":0,"fixed_projection":0})
            },
            service::Service::status,
        )
    }
    pub fn enqueue(
        &mut self,
        events: Vec<event::Event>,
        endpoint: crate::ratio::Ratio,
        frame: u64,
        fixed_frame: u64,
    ) -> Result<Value, String> {
        if self.service.is_none()
            && (!self.keys.is_empty() || self.axes.values().any(|v| *v != 0.0) || !self.focused)
        {
            return Err(
                "timestamped input must be selected before legacy held/axis/focus state".into(),
            );
        }
        self.service
            .get_or_insert_with(|| service::Service::at(endpoint, frame, fixed_frame))
            .enqueue(events)
    }
    pub fn advance(
        &mut self,
        phase: projection::Phase,
        timestamp: crate::ratio::Ratio,
        id: u64,
    ) -> Result<(), String> {
        if let Some(service) = &mut self.service {
            service.advance(phase, timestamp, id, &self.definitions)?;
        }
        Ok(())
    }
}
pub(crate) fn validate_dt(dt: f32) -> Result<(), String> {
    if !dt.is_finite() || !(0.0..=1.0).contains(&dt) {
        Err("dt must be finite in 0..1 seconds".into())
    } else {
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct Random {
    pub state: u64,
}
impl Default for Random {
    fn default() -> Self {
        Self { state: 0x5eed }
    }
}
impl Random {
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub fn range(&mut self, min: f64, max: f64, integer: bool) -> Result<Value, String> {
        if !min.is_finite() || !max.is_finite() || max < min || !(max - min).is_finite() {
            return Err("invalid random range".into());
        }
        if integer {
            if min.fract() != 0.0
                || max.fract() != 0.0
                || min < i32::MIN as f64
                || max > i32::MAX as f64
                || max <= min
            {
                return Err("integer range requires ordered i32 endpoints".into());
            }
            let n = (max - min) as u64;
            let threshold = n.wrapping_neg() % n;
            let mut x = self.next();
            while x < threshold {
                x = self.next();
            }
            Ok(json!(min as i64 + (x % n) as i64))
        } else {
            Ok(json!(
                min + (max - min) * ((self.next() >> 11) as f64 / (1_u64 << 53) as f64)
            ))
        }
    }
}
