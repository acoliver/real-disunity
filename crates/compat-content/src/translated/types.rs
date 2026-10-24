use crate::{CapabilityRequirement, FieldSegment, ResourceKind, SourceRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionPolicy {
    pub schema_version: u32,
    pub profile_id: String,
    pub original_closure_sha256: String,
    pub operations: Vec<SelectionOperation>,
    pub resources: Vec<SelectedResource>,
    pub required_capabilities: Vec<CapabilityRequirement>,
    pub keyboard_only: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionRecord {
    Document {
        source: SourceRef,
    },
    Component {
        source: SourceRef,
        projection: ComponentProjection,
    },
    Metadata {
        path: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentProjection {
    Fields,
    NativeFields,
    ScriptFields,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionEffect {
    pub behavior: String,
    pub lost_effects: Vec<String>,
    pub reason: String,
    pub capability: Option<CapabilityRequirement>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectionOperation {
    ReplaceField {
        id: String,
        record: ExecutionRecord,
        source_sha256: String,
        field: Vec<FieldSegment>,
        expected: Value,
        replacement: Value,
        effect: SelectionEffect,
    },
    ExcludeRecord {
        id: String,
        record: ExecutionRecord,
        source_sha256: String,
        expected_sha256: String,
        effect: SelectionEffect,
    },
}
impl SelectionOperation {
    pub fn id(&self) -> &str {
        match self {
            Self::ReplaceField { id, .. } | Self::ExcludeRecord { id, .. } => id,
        }
    }
    pub fn record(&self) -> &ExecutionRecord {
        match self {
            Self::ReplaceField { record, .. } | Self::ExcludeRecord { record, .. } => record,
        }
    }
    pub fn effect(&self) -> &SelectionEffect {
        match self {
            Self::ReplaceField { effect, .. } | Self::ExcludeRecord { effect, .. } => effect,
        }
    }
    pub fn source_sha256(&self) -> &str {
        match self {
            Self::ReplaceField { source_sha256, .. }
            | Self::ExcludeRecord { source_sha256, .. } => source_sha256,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedResource {
    pub id: String,
    pub resource_kind: ResourceKind,
    pub provider_id: String,
    pub capability: CapabilityRequirement,
    pub bytes_sha256: String,
}
