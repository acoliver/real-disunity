use super::{ComponentProjection, ExecutionRecord};
use crate::{FieldSegment, ImportedScene, ResourceKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "container", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectiveRecord {
    Execution { record: ExecutionRecord },
    AssetMetadata { path: String, guid: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveSelectionObservation {
    pub record: EffectiveRecord,
    pub field: Vec<FieldSegment>,
    pub raw_reference: Value,
    pub resource_kind: ResourceKind,
}

pub(super) fn observe(graph: &ImportedScene) -> Vec<EffectiveSelectionObservation> {
    let mut out = Vec::new();
    for d in &graph.documents {
        scan(
            &mut out,
            ExecutionRecord::Document {
                source: d.source.clone(),
            },
            &d.kind,
            &d.fields,
        );
    }
    for c in &graph.components {
        for (projection, value) in [
            (ComponentProjection::Fields, &c.fields),
            (ComponentProjection::NativeFields, &c.native_fields),
        ] {
            scan(
                &mut out,
                ExecutionRecord::Component {
                    source: c.source.clone(),
                    projection,
                },
                &c.kind,
                value,
            );
        }
        if let Some(script) = &c.script {
            scan(
                &mut out,
                ExecutionRecord::Component {
                    source: c.source.clone(),
                    projection: ComponentProjection::ScriptFields,
                },
                &c.kind,
                &script.fields,
            );
        }
    }
    for (path, value) in &graph.metadata_records {
        scan(
            &mut out,
            ExecutionRecord::Metadata { path: path.clone() },
            "Metadata",
            value,
        );
    }
    for asset in &graph.assets {
        let owner = EffectiveRecord::AssetMetadata {
            path: asset.path.clone(),
            guid: asset.guid.clone(),
        };
        walk(
            &mut out,
            &owner,
            "Metadata",
            &mut Vec::new(),
            &asset.metadata,
        );
    }
    out
}
fn scan(
    out: &mut Vec<EffectiveSelectionObservation>,
    record: ExecutionRecord,
    kind: &str,
    value: &Value,
) {
    walk(
        out,
        &EffectiveRecord::Execution { record },
        kind,
        &mut Vec::new(),
        value,
    );
}
fn walk(
    out: &mut Vec<EffectiveSelectionObservation>,
    record: &EffectiveRecord,
    kind: &str,
    field: &mut Vec<FieldSegment>,
    value: &Value,
) {
    if value
        .as_object()
        .is_some_and(|m| m.keys().any(|k| k.starts_with('$')))
        || value.get("fileID").is_some_and(|v| v.as_i64() != Some(0))
    {
        let name = field
            .iter()
            .rev()
            .find_map(|f| match f {
                FieldSegment::Key(k) => Some(k.as_str()),
                FieldSegment::Index(_) => None,
            })
            .unwrap_or("");
        out.push(EffectiveSelectionObservation {
            record: record.clone(),
            field: field.clone(),
            raw_reference: value.clone(),
            resource_kind: super::closure::field_kind(name, kind),
        });
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                field.push(FieldSegment::Key(key.clone()));
                walk(out, record, kind, field, child);
                field.pop();
            }
        }
        Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                field.push(FieldSegment::Index(
                    u32::try_from(index).expect("bounded selected array"),
                ));
                walk(out, record, kind, field, child);
                field.pop();
            }
        }
        _ => {}
    }
}
