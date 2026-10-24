mod execute;

use crate::{ImportedScene, LocalTransform, SpawnNode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Display};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdmissionPolicy {
    /// Stable component ID or source kind -> explicit omission reason.
    pub ignored_components: BTreeMap<String, String>,
    /// Original script GUID:fileID -> explicit omission reason.
    pub ignored_scripts: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum NativeCommand {
    CreateObject {
        node: Box<SpawnNode>,
        transform_id: String,
    },
    AddComponent {
        id: String,
        object: String,
        kind: String,
        fields: Value,
    },
    Parent {
        object: String,
        parent: Option<String>,
    },
    Transform {
        object: String,
        transform: LocalTransform,
    },
    ConfigureComponent {
        id: String,
        enabled: bool,
        fields: Value,
    },
    Activate {
        object: String,
        active: bool,
    },
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MaterializationPlan {
    pub commands: Vec<NativeCommand>,
    pub skipped_components: BTreeMap<String, String>,
    pub aliases: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MaterializedScene {
    /// Graph node and real component IDs, including implicit Transform component handles.
    pub handles: BTreeMap<String, u64>,
    pub object_ids: BTreeMap<String, u64>,
    pub component_ids: BTreeMap<String, u64>,
    /// Transform graph IDs to owner object handles for transform protocol operations.
    pub transform_objects: BTreeMap<String, u64>,
    pub template_active: BTreeMap<String, bool>,
    pub skipped_components: BTreeMap<String, String>,
}
impl MaterializedScene {
    pub fn binding_payload(&self, world: u64, graph: &ImportedScene) -> Value {
        json!({"world":world,"graph":graph,"handles":self.handles})
    }
}
#[derive(Debug, thiserror::Error)]
pub enum MaterializationError {
    #[error("invalid admission graph {id}: {message}")]
    Graph { id: String, message: String },
    #[error(
        "native materialization command {command}: {message}; host must discard the partial world"
    )]
    Native { command: usize, message: String },
}

pub fn materialization_plan(
    scene: &ImportedScene,
    policy: &AdmissionPolicy,
) -> Result<MaterializationPlan, MaterializationError> {
    crate::admission::plan(scene, policy)
}
pub fn materialize<E: Display>(
    scene: &ImportedScene,
    policy: &AdmissionPolicy,
    request: impl FnMut(Value) -> Result<Value, E>,
) -> Result<MaterializedScene, MaterializationError> {
    materialization_plan(scene, policy)?.execute(request)
}
