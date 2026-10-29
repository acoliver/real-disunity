//! Estimated run times that let `partition` balance shards by work rather than by count.
//!
//! The weights live in `crates/xtask/gpu-test-weights.toml`. They only affect which shard runs a
//! test; `verify` still proves that every test runs exactly once, so a stale weight costs balance
//! and never coverage.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::Unit;

pub const FILE: &str = "crates/xtask/gpu-test-weights.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weights {
    default_test: u64,
    default_unit: u64,
    tests: BTreeMap<String, u64>,
    units: BTreeMap<String, u64>,
}

fn table(value: &toml::Value, key: &str) -> Result<BTreeMap<String, u64>, String> {
    let Some(entries) = value.get(key) else {
        return Ok(BTreeMap::new());
    };
    let entries = entries
        .as_table()
        .ok_or_else(|| format!("{FILE}: `{key}` must be a table"))?;
    entries
        .iter()
        .map(|(name, seconds)| Ok((name.clone(), seconds_of(seconds, name)?)))
        .collect()
}

fn seconds_of(value: &toml::Value, name: &str) -> Result<u64, String> {
    value
        .as_integer()
        .and_then(|seconds| u64::try_from(seconds).ok())
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| format!("{FILE}: `{name}` must be a positive whole number of seconds"))
}

fn required(value: &toml::Value, key: &str) -> Result<u64, String> {
    let field = value
        .get(key)
        .ok_or_else(|| format!("{FILE}: missing `{key}`"))?;
    seconds_of(field, key)
}

fn unit_key(unit: &Unit) -> String {
    format!("{} {}", unit.kind.flag().trim_start_matches('-'), unit.name)
}

impl Weights {
    /// Every test and every unit weighs one second. Partitioning then deals round robin.
    #[cfg(test)]
    pub fn uniform() -> Self {
        Self {
            default_test: 1,
            default_unit: 1,
            tests: BTreeMap::new(),
            units: BTreeMap::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(text).map_err(|e| format!("{FILE}: {e}"))?;
        if let Some(table) = value.as_table() {
            let known = ["default_test", "default_unit", "tests", "units"];
            if let Some(key) = table.keys().find(|key| !known.contains(&key.as_str())) {
                return Err(format!("{FILE}: unknown key `{key}`"));
            }
        }
        Ok(Self {
            default_test: required(&value, "default_test")?,
            default_unit: required(&value, "default_unit")?,
            tests: table(&value, "tests")?,
            units: table(&value, "units")?,
        })
    }

    pub fn read(root: &Path) -> Result<Self, String> {
        let path = root.join(FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::parse(&text)
    }

    pub fn test(&self, name: &str) -> u64 {
        self.tests.get(name).copied().unwrap_or(self.default_test)
    }

    pub fn unit(&self, unit: &Unit) -> u64 {
        self.units
            .get(&unit_key(unit))
            .copied()
            .unwrap_or(self.default_unit)
    }

    /// Weighted entries that match no listed test or unit, for `ci-gpu-plan` to report.
    pub fn unmatched(&self, units: &BTreeSet<Unit>, names: &BTreeSet<String>) -> Vec<String> {
        let unit_keys: BTreeSet<_> = units.iter().map(unit_key).collect();
        let stale_units = self.units.keys().filter(|key| !unit_keys.contains(*key));
        let stale_tests = self.tests.keys().filter(|name| !names.contains(*name));
        stale_units.chain(stale_tests).cloned().collect()
    }
}
