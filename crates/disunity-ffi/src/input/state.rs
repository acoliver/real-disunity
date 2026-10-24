use super::{
    axes::Axis,
    event::{Action, Event, Source},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default)]
struct Device {
    keys: BTreeSet<String>,
    axes: BTreeMap<String, f32>,
}
#[derive(Clone)]
pub(super) struct State {
    devices: BTreeMap<Source, Device>,
    pub focused: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            devices: BTreeMap::new(),
            focused: true,
        }
    }
}
impl State {
    pub fn keys(&self) -> BTreeSet<String> {
        self.devices
            .values()
            .flat_map(|d| d.keys.iter().cloned())
            .collect()
    }
    pub fn axes(&self, definitions: &BTreeMap<String, Axis>) -> BTreeMap<String, f32> {
        let keys = self.keys();
        let mut axes: BTreeMap<_, _> = definitions
            .iter()
            .map(|(name, axis)| (name.clone(), axis.raw(&keys)))
            .collect();
        for device in self.devices.values() {
            axes.extend(device.axes.clone());
        }
        axes
    }
    pub fn apply(&mut self, event: &Event) -> Result<(), String> {
        if let Action::Focus(focused) = event.action {
            self.focused = focused;
            if !focused {
                self.devices.clear();
            }
            return Ok(());
        }
        if !self.focused {
            return Ok(());
        }
        let device = self.devices.entry(event.source).or_default();
        match &event.action {
            Action::Key(key, true) => {
                device.keys.insert(key.clone());
            }
            Action::Key(key, false) => {
                device.keys.remove(key);
            }
            Action::Axis(name, value) => {
                device.axes.insert(name.clone(), *value);
            }
            Action::Focus(_) => unreachable!(),
        }
        if self.keys().len() > 256
            || self.devices.values().map(|d| d.axes.len()).sum::<usize>() > 256
        {
            return Err("normalized held key/axis budget exceeded".into());
        }
        Ok(())
    }
}
