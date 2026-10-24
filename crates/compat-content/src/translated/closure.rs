use super::{GameSourceQualification, SelectedResource, SelectionPolicy};
use crate::{ImportedScene, ResourceKind, TrustedProviderInventory};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn providers(
    policy: &SelectionPolicy,
    providers: &TrustedProviderInventory,
) -> Result<(), String> {
    super::scope::validate_values(policy)?;
    if policy.keyboard_only
        && !policy
            .required_capabilities
            .iter()
            .any(|c| c.id == crate::CapabilityId::CanonicalKeyboardSnapshot)
    {
        return Err("keyboard selection requires the qualified snapshot input provider".into());
    }
    for capability in policy.required_capabilities.iter().chain(
        policy
            .operations
            .iter()
            .filter_map(|op| op.effect().capability.as_ref()),
    ) {
        capability.validate()?;
        if providers.capability(capability).is_none() {
            return Err(format!(
                "qualified provider missing for {:?}",
                capability.id
            ));
        }
    }
    let mut ids = BTreeSet::new();
    for resource in &policy.resources {
        resource.capability.validate()?;
        if resource.id.is_empty()
            || !ids.insert(&resource.id)
            || resource.resource_kind == ResourceKind::Unknown
            || resource.capability.resource_kind != resource.resource_kind
        {
            return Err("invalid exact typed selected resource".into());
        }
        let binding = providers
            .bindings()
            .get(&resource.provider_id)
            .ok_or("resource provider absent")?;
        if binding.resource_kind != resource.resource_kind
            || !binding.capabilities.contains(&resource.capability)
        {
            return Err("resource provider kind/capability mismatch".into());
        }
        let bytes = providers
            .resource_bytes(&resource.provider_id, &resource.id)
            .ok_or("qualified resource bytes absent")?;
        if crate::dependency::hash(bytes) != resource.bytes_sha256 {
            return Err("qualified resource bytes changed".into());
        }
    }
    Ok(())
}
pub(super) fn graph(
    graph: &ImportedScene,
    policy: &SelectionPolicy,
    source: &GameSourceQualification,
) -> Result<(), String> {
    let scanner = Scanner {
        graph,
        resources: &policy.resources,
        source,
    };
    for doc in &graph.documents {
        if doc.kind == "MonoBehaviour"
            && !graph
                .components
                .iter()
                .any(|c| c.source == doc.source && c.script.is_some())
        {
            return Err("selected unknown MonoBehaviour requires qualified type mapping".into());
        }
        scanner.walk(
            &doc.fields,
            "",
            0,
            &Owner {
                guid: &doc.source.guid,
                kind: &doc.kind,
            },
            None,
        )?;
    }
    for component in &graph.components {
        scanner.component(component)?;
    }
    for record in graph.metadata_records.values() {
        scanner.walk(
            record,
            "",
            0,
            &Owner {
                guid: &graph.scene_guid,
                kind: "Metadata",
            },
            None,
        )?;
    }
    for asset in &graph.assets {
        scanner.walk(
            &asset.metadata,
            "",
            0,
            &Owner {
                guid: &asset.guid,
                kind: "Metadata",
            },
            None,
        )?;
    }
    Ok(())
}
struct Owner<'a> {
    guid: &'a str,
    kind: &'a str,
}
struct Scanner<'a> {
    graph: &'a ImportedScene,
    resources: &'a [SelectedResource],
    source: &'a GameSourceQualification,
}
impl Scanner<'_> {
    fn component(&self, component: &crate::ImportedComponent) -> Result<(), String> {
        let owner = Owner {
            guid: &component.source.guid,
            kind: &component.kind,
        };
        self.walk(&component.fields, "", 0, &owner, None)?;
        self.walk(&component.native_fields, "", 0, &owner, None)?;
        if let Some(script) = &component.script {
            self.walk(&script.fields, "", 0, &owner, None)?;
        }
        Ok(())
    }
    fn walk(
        &self,
        value: &Value,
        field: &str,
        depth: usize,
        owner: &Owner<'_>,
        inherited_kind: Option<ResourceKind>,
    ) -> Result<(), String> {
        if depth > 96 {
            return Err("selected field nesting exceeds 96".into());
        }
        Self::pipeline(value, field)?;
        let expected = inherited_kind.or_else(|| {
            let kind = field_kind(field, owner.kind);
            (kind != ResourceKind::Unknown).then_some(kind)
        });
        self.reference(value, field, owner, expected)?;
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    self.walk(child, key, depth + 1, owner, expected)?;
                }
            }
            Value::Array(array) => {
                for child in array {
                    self.walk(child, field, depth + 1, owner, expected)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn pipeline(value: &Value, field: &str) -> Result<(), String> {
        if matches!(field, "customRenderPipeline" | "m_CustomRenderPipeline")
            && !value.is_null()
            && value["fileID"].as_i64() != Some(0)
        {
            return Err("every selected quality tier must use the canonical unlit pipeline".into());
        }
        if field == "m_RenderPipelineGlobalSettingsMap"
            && !value.is_null()
            && !value.as_object().is_some_and(serde_json::Map::is_empty)
            && !value.as_array().is_some_and(Vec::is_empty)
        {
            return Err("selected original pipeline global settings remain".into());
        }
        Ok(())
    }
    fn reference(
        &self,
        value: &Value,
        field: &str,
        owner: &Owner<'_>,
        expected: Option<ResourceKind>,
    ) -> Result<(), String> {
        let Some(map) = value.as_object() else {
            return Ok(());
        };
        let markers: Vec<_> = map.keys().filter(|key| key.starts_with('$')).collect();
        if markers.len() > 1 {
            return Err("ambiguous selected reference markers".into());
        }
        if let Some(marker) = markers.first() {
            let id = value[*marker]
                .as_str()
                .ok_or("malformed selected reference")?;
            match marker.as_str() {
                "$ref" => self.target(id, field, owner.kind, expected)?,
                "$resource" if map.len() == 1 => self.resource(id, field, owner.kind, expected)?,
                "$type"
                    if field_kind(field, owner.kind) == ResourceKind::Script
                        && self.source.has_type(id)
                        && map
                            .keys()
                            .all(|k| matches!(k.as_str(), "$type" | "guid" | "file_id")) => {}
                _ => return Err(format!("unqualified selected reference {field}: {value}")),
            }
            self.identity(value, id, marker)?;
        } else if let Some(file) = value.get("fileID")
            && file.as_i64() != Some(0)
        {
            let file = file.as_i64().ok_or("malformed serialized selected edge")?;
            let guid = value
                .get("guid")
                .map_or(Some(owner.guid), Value::as_str)
                .ok_or("malformed selected GUID")?;
            self.target(&format!("{guid}:{file}"), field, owner.kind, expected)?;
        }
        Ok(())
    }
    fn identity(&self, value: &Value, target: &str, marker: &str) -> Result<(), String> {
        if marker != "$ref" {
            return Ok(());
        }
        if let (Some(guid), Some(file)) = (value.get("guid"), value.get("file_id")) {
            let guid = guid.as_str().ok_or("invalid selected GUID")?;
            let file = file.as_i64().ok_or("invalid selected fileID")?;
            let key = format!("{guid}:{file}");
            let matching = target == key
                || self.graph.aliases.get(&key).is_some_and(|v| v == target)
                || self.graph.documents.iter().any(|d| {
                    d.source.key() == target && d.source.guid == guid && d.source.file_id == file
                });
            if !matching {
                return Err("selected effective target identity mismatch".into());
            }
        } else {
            return Err("selected reference requires retained GUID/fileID identity".into());
        }
        Ok(())
    }
    fn target(
        &self,
        id: &str,
        field: &str,
        owner_kind: &str,
        expected: Option<ResourceKind>,
    ) -> Result<(), String> {
        let target = self.graph.aliases.get(id).map_or(id, String::as_str);
        let doc = self
            .graph
            .documents
            .iter()
            .find(|d| d.source.key() == target)
            .ok_or_else(|| format!("selected edge target absent: {id}"))?;
        let expected = expected.unwrap_or_else(|| field_kind(field, owner_kind));
        let valid = match expected {
            ResourceKind::Material => doc.class_id == 21 && doc.kind == "Material",
            ResourceKind::Shader => doc.class_id == 48 && doc.kind == "Shader",
            ResourceKind::PhysicsMaterial2D => {
                doc.class_id == 62 && doc.kind == "PhysicsMaterial2D"
            }
            ResourceKind::Font | ResourceKind::Texture | ResourceKind::Script => false,
            _ => true,
        };
        if !valid {
            return Err(format!(
                "selected edge target has wrong type: {field} -> {id}"
            ));
        }
        if doc.kind == "MonoBehaviour"
            && !self
                .graph
                .components
                .iter()
                .any(|c| c.id == target && c.script.is_some())
        {
            return Err("selected unknown MonoBehaviour has no qualified managed mapping".into());
        }
        Ok(())
    }
    fn resource(
        &self,
        id: &str,
        field: &str,
        owner_kind: &str,
        expected: Option<ResourceKind>,
    ) -> Result<(), String> {
        let resource = self
            .resources
            .iter()
            .find(|r| r.id == id)
            .ok_or("unlisted selected resource")?;
        let expected = expected.unwrap_or_else(|| field_kind(field, owner_kind));
        if expected == ResourceKind::Unknown || resource.resource_kind != expected {
            return Err("selected resource use has wrong/unknown type".into());
        }
        Ok(())
    }
}
pub(super) fn field_kind(field: &str, owner_kind: &str) -> ResourceKind {
    match field {
        "m_Shader" | "m_AlwaysIncludedShaders" => ResourceKind::Shader,
        "m_Material"
            if matches!(
                owner_kind,
                "Rigidbody2D"
                    | "BoxCollider2D"
                    | "CircleCollider2D"
                    | "PolygonCollider2D"
                    | "TilemapCollider2D"
            ) =>
        {
            ResourceKind::PhysicsMaterial2D
        }
        "m_Material" | "m_Materials" | "m_SpritesDefaultMaterial" => ResourceKind::Material,
        "m_Font" => ResourceKind::Font,
        "m_Texture" | "m_MainTexture" | "m_SpotCookie" | "m_Sprite" => ResourceKind::Texture,
        "m_Script" => ResourceKind::Script,
        _ => ResourceKind::Unknown,
    }
}
