use super::{ExecutionRecord, SelectionOperation};
use crate::{CapabilityId, ContentReceipt, FieldSegment, ResourceKind};
use serde_json::Value;

pub(super) fn validate(receipt: &ContentReceipt, op: &SelectionOperation) -> Result<(), String> {
    let Some(capability) = &op.effect().capability else {
        return Ok(());
    };
    capability.validate()?;
    let SelectionOperation::ReplaceField { record, field, .. } = op else {
        return Err("typed capability cannot exclude an entire record".into());
    };
    let name = field
        .iter()
        .rev()
        .find_map(|s| match s {
            FieldSegment::Key(key) => Some(key.as_str()),
            FieldSegment::Index(_) => None,
        })
        .ok_or("selection requires an exact typed field")?;
    let kind = match record {
        ExecutionRecord::Metadata { path } => {
            metadata(path, field, capability.id)?;
            "Metadata"
        }
        ExecutionRecord::Document { source } | ExecutionRecord::Component { source, .. } => {
            let kind = receipt
                .graph()
                .documents
                .iter()
                .find(|d| d.source == *source)
                .map(|d| d.kind.as_str())
                .ok_or("selected document absent")?;
            resource_field(kind, field, capability.id)?;
            kind
        }
    };
    if capability.id != CapabilityId::CanonicalUnlitQuads
        && super::closure::field_kind(name, kind) != capability.resource_kind
    {
        return Err("capability does not cover the exact selected field kind".into());
    }
    if capability.id == CapabilityId::CanonicalUnlitQuads
        && !matches!(record, ExecutionRecord::Metadata { .. })
    {
        return Err("unlit quads capability covers graphics/quality settings only".into());
    }
    if capability.resource_kind == ResourceKind::Input {
        return Err(
            "input importer exclusion requires a separately supported exact record contract".into(),
        );
    }
    Ok(())
}

fn resource_field(
    kind: &str,
    field: &[FieldSegment],
    capability: CapabilityId,
) -> Result<(), String> {
    let [FieldSegment::Key(name), rest @ ..] = field else {
        return Err("resource selection requires an exact direct field".into());
    };
    let direct = rest.is_empty() || matches!(rest, [FieldSegment::Index(_)]);
    let valid = match capability {
        CapabilityId::CanonicalUnlitShader => kind == "Material" && name == "m_Shader",
        CapabilityId::CanonicalUnlitMaterial => {
            matches!(kind, "SpriteRenderer" | "MeshRenderer")
                && matches!(name.as_str(), "m_Material" | "m_Materials")
        }
        CapabilityId::AuthoredSprite => kind == "SpriteRenderer" && name == "m_Sprite",
        CapabilityId::MappedFont => kind == "MonoBehaviour" && name == "m_Font",
        CapabilityId::MappedTileType => kind == "MonoBehaviour" && name == "m_Script",
        _ => false,
    };
    if !direct || !valid {
        return Err("capability does not cover the selected record/field".into());
    }
    Ok(())
}
fn metadata(path: &str, field: &[FieldSegment], capability: CapabilityId) -> Result<(), String> {
    let [
        FieldSegment::Key(documents),
        FieldSegment::Index(_),
        rest @ ..,
    ] = field
    else {
        return Err("settings selection must retain the complete document collection".into());
    };
    if documents != "documents" {
        return Err("settings selection requires an exact document field".into());
    }
    let valid = match (path, rest) {
        ("ProjectSettings/GraphicsSettings.asset", [FieldSegment::Key(name)]) => matches!(
            name.as_str(),
            "m_AlwaysIncludedShaders"
                | "m_CustomRenderPipeline"
                | "m_RenderPipelineGlobalSettingsMap"
                | "m_SpritesDefaultMaterial"
                | "m_Deferred"
                | "m_DeferredReflections"
                | "m_ScreenSpaceShadows"
                | "m_DepthNormals"
                | "m_MotionVectors"
                | "m_LightHalo"
                | "m_LensFlare"
        ),
        (
            "ProjectSettings/QualitySettings.asset",
            [
                FieldSegment::Key(tiers),
                FieldSegment::Index(_),
                FieldSegment::Key(name),
            ],
        ) => {
            tiers == "m_QualitySettings"
                && name == "customRenderPipeline"
                && capability == CapabilityId::CanonicalUnlitQuads
        }
        _ => false,
    };
    if !valid {
        return Err("capability does not authorize redaction of this settings field".into());
    }
    Ok(())
}

fn validate_metadata_value(
    path: &str,
    field: &[FieldSegment],
    value: &Value,
) -> Result<(), String> {
    let name = field
        .iter()
        .rev()
        .find_map(|segment| match segment {
            FieldSegment::Key(name) => Some(name.as_str()),
            FieldSegment::Index(_) => None,
        })
        .ok_or("settings selection requires a typed field")?;
    match name {
        "m_AlwaysIncludedShaders" => {
            let shaders = value
                .as_array()
                .ok_or("shader list replacement must be an array")?;
            for shader in shaders {
                validate_shader_value(shader)?;
            }
        }
        "m_CustomRenderPipeline" | "customRenderPipeline" => {
            if !canonical_null(value) {
                return Err("pipeline replacement must select the canonical null pipeline".into());
            }
        }
        "m_RenderPipelineGlobalSettingsMap" => {
            if !value.is_null()
                && !value.as_object().is_some_and(serde_json::Map::is_empty)
                && !value.as_array().is_some_and(Vec::is_empty)
            {
                return Err("pipeline global settings replacement must be empty".into());
            }
        }
        "m_SpritesDefaultMaterial" => {
            if !value.is_null() {
                validate_typed_reference(value, ResourceKind::Material)?;
            }
        }
        _ if path == "ProjectSettings/GraphicsSettings.asset" => {
            if !value.is_boolean() && !value.is_number() {
                return Err("graphics setting replacement must be a scalar value".into());
            }
        }
        _ => return Err("unsupported settings replacement value contract".into()),
    }
    Ok(())
}

fn canonical_null(value: &Value) -> bool {
    value.is_null()
        || value.as_object().is_some_and(|map| {
            map.len() == 1 && map.get("fileID").and_then(Value::as_i64) == Some(0)
        })
}

fn validate_shader_value(value: &Value) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    validate_typed_reference(value, ResourceKind::Shader)
}

fn validate_typed_reference(value: &Value, kind: ResourceKind) -> Result<(), String> {
    let map = value
        .as_object()
        .ok_or("typed reference must be an object or null")?;
    if let Some(resource) = map.get("$resource") {
        if map.len() == 1 && resource.as_str().is_some_and(|id| !id.is_empty()) {
            return Ok(());
        }
        return Err("malformed selected resource reference".into());
    }
    if let Some(target) = map.get("$ref") {
        if kind == ResourceKind::Shader
            && map.len() == 3
            && target.as_str().is_some_and(|id| !id.is_empty())
            && map
                .get("guid")
                .and_then(Value::as_str)
                .is_some_and(valid_guid)
            && map
                .get("file_id")
                .and_then(Value::as_i64)
                .is_some_and(|id| id != 0)
        {
            return Ok(());
        }
        return Err("malformed or unsupported typed projected reference".into());
    }
    let allowed: &[&str] = &["fileID", "guid", "type"];
    if map.keys().any(|key| !allowed.contains(&key.as_str()))
        || map
            .get("fileID")
            .and_then(Value::as_i64)
            .is_none_or(|id| id == 0)
        || map
            .get("guid")
            .is_some_and(|guid| guid.as_str().is_none_or(|id| !valid_guid(id)))
        || map.get("type").is_some_and(|kind| kind.as_i64().is_none())
    {
        return Err("malformed native typed reference".into());
    }
    Ok(())
}

fn valid_guid(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn validate_values(policy: &super::SelectionPolicy) -> Result<(), String> {
    for operation in &policy.operations {
        if let SelectionOperation::ReplaceField {
            record: ExecutionRecord::Metadata { path },
            field,
            replacement,
            ..
        } = operation
            && matches!(
                path.as_str(),
                "ProjectSettings/GraphicsSettings.asset" | "ProjectSettings/QualitySettings.asset"
            )
        {
            validate_metadata_value(path, field, replacement)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_null_cannot_hide_additional_required_edges() {
        assert!(canonical_null(&Value::Null));
        assert!(canonical_null(&json!({"fileID":0})));
        assert!(!canonical_null(&json!({"fileID":0,"nested":{"fileID":42}})));
    }
}
