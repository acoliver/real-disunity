use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Axis {
    negative_keys: BTreeSet<String>,
    positive_keys: BTreeSet<String>,
    sensitivity: f32,
    gravity: f32,
    dead: f32,
    snap: bool,
    invert: bool,
}
impl Default for Axis {
    fn default() -> Self {
        Self {
            negative_keys: BTreeSet::new(),
            positive_keys: BTreeSet::new(),
            sensitivity: 3.0,
            gravity: 3.0,
            dead: 0.001,
            snap: false,
            invert: false,
        }
    }
}
impl Axis {
    pub(super) fn raw(&self, keys: &BTreeSet<String>) -> f32 {
        let positive = !self.positive_keys.is_disjoint(keys);
        let negative = !self.negative_keys.is_disjoint(keys);
        let raw = i32::from(positive) - i32::from(negative);
        let value = raw as f32 * if self.invert { -1.0 } else { 1.0 };
        if value.abs() <= self.dead { 0.0 } else { value }
    }
    fn validate(&self) -> Result<(), String> {
        if !self.sensitivity.is_finite()
            || self.sensitivity < 0.0
            || !self.gravity.is_finite()
            || self.gravity < 0.0
            || !self.dead.is_finite()
            || !(0.0..=1.0).contains(&self.dead)
            || self.negative_keys.len() + self.positive_keys.len() > 32
            || self
                .negative_keys
                .iter()
                .chain(&self.positive_keys)
                .any(|k| !valid_name(k))
        {
            return Err("invalid keyboard axis settings".into());
        }
        Ok(())
    }
}
pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}
pub(super) fn validate(axes: &BTreeMap<String, Axis>) -> Result<(), String> {
    if axes.len() > 256 || axes.keys().any(|k| !valid_name(k)) {
        return Err("axis configuration exceeds name/count budget".into());
    }
    for axis in axes.values() {
        axis.validate()?;
    }
    Ok(())
}
