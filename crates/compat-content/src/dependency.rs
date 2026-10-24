mod classify;
mod effective;
mod engine;
mod observe;
mod policy;
mod projection;
mod proof;
mod provider;
mod resource;
mod wire;

pub use engine::{AdmittedContent, DependencyAdmissionError};
pub use policy::{
    Confidence, DependencyDisposition, DependencyPolicy, DependencyRule, EvidenceRef, Fidelity,
};
pub use proof::{AdmissionOutcome, AdmissionProof, DependencyClassification, ReferenceDecision};
pub use provider::{
    CapabilityId, CapabilityRequirement, ProviderBinding, TrustedProvider, TrustedProviderInventory,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(
    tag = "segment",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FieldSegment {
    Key(String),
    Index(u32),
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "container", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerIdentity {
    SettingsDocument {
        document_index: u32,
        class_id: i32,
        file_id: i64,
        kind: String,
    },
    AssetDocument {
        asset_guid: String,
        document_index: u32,
        class_id: i32,
        file_id: i64,
        kind: String,
    },
    ExpandedDocument {
        asset_guid: String,
        document_index: u32,
        class_id: i32,
        file_id: i64,
        kind: String,
        instance: String,
    },
    AssetMetadata {
        asset_guid: String,
        importer_kind: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLocator {
    pub source_path: String,
    pub owner: OwnerIdentity,
    pub field: Vec<FieldSegment>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializedReference {
    pub guid: String,
    pub file_id: i64,
    pub reference_type: Option<i32>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceKind {
    Shader,
    Material,
    #[serde(rename = "physics_material_2d")]
    PhysicsMaterial2D,
    Font,
    Texture,
    Script,
    ImporterScript,
    Object,
    Unknown,
    RenderSettings,
    Input,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReferenceResolution {
    ResolvedTarget { target: String },
    BuiltinMapping { provider_id: String },
    ManagedTypeMapping { type_name: String },
    Unresolved { code: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocatedReference {
    pub locator: ReferenceLocator,
    pub source_sha256: String,
    pub reference: Option<SerializedReference>,
    pub raw_reference: Value,
    pub resource_kind: ResourceKind,
    pub resolution: ReferenceResolution,
}

pub(crate) use observe::observe;
pub(crate) fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn identity(value: &impl Serialize) -> String {
    hash(&serde_json::to_vec(value).expect("serializable admission contract"))
}
