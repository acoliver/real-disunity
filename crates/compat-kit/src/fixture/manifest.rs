use crate::files::{self, Result};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceabilityManifest {
    schema: String,
    #[serde(rename = "counts")]
    _counts: BTreeMap<String, Value>,
    #[serde(rename = "cases")]
    _cases: Vec<Value>,
    #[serde(rename = "requirements")]
    _requirements: Vec<Value>,
    #[serde(rename = "evidence")]
    _evidence: Vec<Value>,
    #[serde(rename = "variants")]
    _variants: Vec<Value>,
    #[serde(rename = "historical")]
    _historical: BTreeMap<String, Value>,
    #[serde(rename = "recorded_runs")]
    _recorded_runs: Vec<Value>,
}

pub(super) fn is_traceability(path: &Path) -> Result<bool> {
    if path
        .file_name()
        .is_none_or(|name| name != "compatibility-catalog.json")
    {
        return Ok(false);
    }
    let manifest: TraceabilityManifest = files::read_json(path)
        .map_err(|error| format!("invalid traceability manifest {}: {error}", path.display()))?;
    if manifest.schema != "compatibility-traceability/1" {
        return Err(format!(
            "unsupported traceability manifest schema: {}",
            manifest.schema
        ));
    }
    Ok(true)
}
