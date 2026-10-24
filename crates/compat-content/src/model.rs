use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use worldrunner_asset_id::{ContentHash, LogicalAssetId};

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SourceRef {
    pub guid: String,
    pub file_id: i64,
    #[serde(default)]
    pub instance: String,
}
impl SourceRef {
    pub fn key(&self) -> String {
        if self.instance.is_empty() {
            format!("{}:{}", self.guid, self.file_id)
        } else {
            format!("{}:{}@{}", self.guid, self.file_id, self.instance)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportedScene {
    pub scene: String,
    pub scene_guid: String,
    pub nodes: Vec<SpawnNode>,
    pub components: Vec<ImportedComponent>,
    pub assets: Vec<ImportedAsset>,
    pub documents: Vec<ImportedDocument>,
    pub aliases: BTreeMap<String, String>,
    pub diagnostics: Vec<Diagnostic>,
    pub source_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub metadata_records: BTreeMap<String, Value>,
    #[serde(default)]
    pub fallback_sprites: Vec<FallbackSprite>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnNode {
    pub id: String,
    pub source: SourceRef,
    pub name: String,
    pub tag: String,
    pub active: bool,
    pub layer: i64,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub components: Vec<String>,
    pub transform: LocalTransform,
    pub template: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalTransform {
    pub local_position: [f64; 3],
    pub local_rotation: [f64; 4],
    pub local_scale: [f64; 3],
}
impl Default for LocalTransform {
    fn default() -> Self {
        Self {
            local_position: [0.; 3],
            local_rotation: [0., 0., 0., 1.],
            local_scale: [1.; 3],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportedComponent {
    pub id: String,
    pub source: SourceRef,
    pub object: Option<String>,
    pub kind: String,
    pub class_id: i32,
    pub enabled: bool,
    pub fields: Value,
    pub native_kind: Option<String>,
    pub native_fields: Value,
    pub script: Option<ScriptBinding>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptBinding {
    pub guid: String,
    pub path: String,
    pub type_name: String,
    pub fields: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportedDocument {
    pub source: SourceRef,
    pub path: String,
    pub class_id: i32,
    pub kind: String,
    pub fields: Value,
    pub template: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportedAsset {
    pub guid: String,
    pub logical_id: LogicalAssetId,
    pub path: String,
    pub kind: AssetKind,
    pub content_hash: ContentHash,
    pub sha256: String,
    pub byte_length: u64,
    pub metadata: Value,
    pub texture_size: Option<[u32; 2]>,
    pub sprites: Vec<SpriteAsset>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpriteAsset {
    pub file_id: i64,
    pub name: String,
    /// Pixel rectangle [x, y, width, height], origin at the source image bottom-left.
    pub rect: [f64; 4],
    pub pivot: [f64; 2],
    pub pixels_per_unit: f64,
    pub border: [f64; 4],
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AssetKind {
    Scene,
    Prefab,
    Script,
    ScriptableObject,
    Texture,
    Material,
    Mesh,
    Font,
    Audio,
    Data,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: String,
    pub code: String,
    pub path: String,
    pub guid: String,
    pub file_id: Option<i64>,
    pub component: Option<String>,
    pub field: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportProfile {
    pub name: String,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_documents: usize,
    pub max_assets: usize,
    pub max_catalog_entries: usize,
    pub max_prefab_depth: usize,
    pub prefab_source_fields: Vec<String>,
    pub correspondence_fields: Vec<String>,
    pub instance_fields: Vec<String>,
    /// Source script GUID -> serialized field alias -> current managed field name.
    pub field_aliases: BTreeMap<String, BTreeMap<String, String>>,
    /// Script GUID -> stale serialized field -> explicit exclusion reason.
    pub field_exclusions: BTreeMap<String, BTreeMap<String, String>>,
    /// Script GUIDs for package UI classes whose source is outside the selected project.
    pub script_types: BTreeMap<String, String>,
    /// "guid:fileID" or GUID -> explicit host replacement name.
    pub builtin_replacements: BTreeMap<String, String>,
}
impl Default for ImportProfile {
    fn default() -> Self {
        Self {
            name: "unity-text-modern-and-2017".into(),
            max_file_bytes: 32 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_documents: 100_000,
            max_assets: 10_000,
            max_catalog_entries: 100_000,
            max_prefab_depth: 32,
            prefab_source_fields: vec!["m_SourcePrefab".into(), "m_ParentPrefab".into()],
            correspondence_fields: vec![
                "m_CorrespondingSourceObject".into(),
                "m_PrefabParentObject".into(),
            ],
            instance_fields: vec!["m_PrefabInstance".into(), "m_PrefabInternal".into()],
            field_aliases: BTreeMap::new(),
            field_exclusions: BTreeMap::new(),
            script_types: BTreeMap::new(),
            builtin_replacements: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportSidecar {
    pub field_overrides: Vec<FieldOverride>,
    pub reference_substitutions: Vec<ReferenceSubstitution>,
    pub fallback_sprites: Vec<FallbackSprite>,
    pub tile_collider_fallbacks: Vec<TileColliderFallback>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileColliderFallback {
    pub guid: String,
    pub file_id: i64,
    pub geometry: TileCollisionGeometry,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileCollisionGeometry {
    SpriteBounds,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldOverride {
    pub guid: String,
    pub file_id: i64,
    pub property_path: String,
    #[serde(default)]
    pub precondition: Option<OverlayPrecondition>,
    pub value: Value,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferenceSubstitution {
    pub guid: String,
    pub file_id: Option<i64>,
    /// A raw Unity reference or explicit {"$builtin":"authored-visual-name"}.
    pub replacement: Value,
    pub reason: String,
}
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{path}: {message}")]
    Source { path: String, message: String },
    #[error("import bound exceeded: {0}")]
    Limit(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FallbackSprite {
    pub name: String,
    pub texture_size: [u32; 2],
    pub rgba: [u8; 4],
    pub sprite: SpriteAsset,
    pub reason: String,
}
impl FallbackSprite {
    pub fn rgba8(&self) -> Result<Vec<u8>, ImportError> {
        crate::sprites::fallback_pixels(self)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptSource {
    pub path: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayPrecondition {
    pub schema_version: u32,
    pub class_id: i32,
    pub expected_value: Value,
    /// Optional exact file observation. Omit to allow unrelated compatible additions.
    #[serde(default)]
    pub source_sha256: Option<String>,
}
