use super::{
    FieldSegment, LocatedReference, OwnerIdentity, ReferenceLocator, ReferenceResolution,
    ResourceKind, SerializedReference, Value,
};
use crate::{ContentReceipt, ImportError, source_io::error};

pub(crate) fn observe(receipt: &ContentReceipt) -> Result<Vec<LocatedReference>, ImportError> {
    let mut observations = Vec::new();
    for (path, bytes) in receipt.sources() {
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        if std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("meta"))
        {
            let fields = crate::yaml::parse_yaml_value(text).map_err(|e| error(path, e))?;
            let guid = fields["guid"].as_str().unwrap_or("");
            let importer = fields
                .as_object()
                .and_then(|m| m.keys().find(|k| k.ends_with("Importer")))
                .cloned()
                .unwrap_or_default();
            let owner = OwnerIdentity::AssetMetadata {
                asset_guid: guid.into(),
                importer_kind: importer,
            };
            let locator = ReferenceLocator {
                source_path: path.clone(),
                owner,
                field: vec![],
            };
            walk(receipt, &locator, &fields, &mut observations);
            if let Some(projected) = resolved_value(receipt, &locator) {
                super::projection::scan(receipt, &locator, &fields, projected, &mut observations);
            }
        } else if text.lines().any(|line| line.starts_with("--- !u!")) {
            documents(receipt, path, text, &mut observations)?;
        }
    }
    observations.sort_by(|a, b| a.locator.cmp(&b.locator));
    Ok(observations)
}
fn documents(
    receipt: &ContentReceipt,
    path: &str,
    text: &str,
    out: &mut Vec<LocatedReference>,
) -> Result<(), ImportError> {
    let docs = crate::yaml::parse_unity_yaml(text, 100_000).map_err(|e| error(path, e))?;
    let guid = receipt
        .graph()
        .assets
        .iter()
        .find(|a| a.path == path)
        .map_or("", |a| a.guid.as_str());
    for (index, doc) in docs.into_iter().enumerate() {
        let document_index = u32::try_from(index).map_err(|e| error(path, e.to_string()))?;
        let owner = if path.starts_with("ProjectSettings/") {
            OwnerIdentity::SettingsDocument {
                document_index,
                class_id: doc.class_id,
                file_id: doc.file_id,
                kind: doc.kind,
            }
        } else {
            OwnerIdentity::AssetDocument {
                asset_guid: guid.into(),
                document_index,
                class_id: doc.class_id,
                file_id: doc.file_id,
                kind: doc.kind,
            }
        };
        let locator = ReferenceLocator {
            source_path: path.into(),
            owner,
            field: vec![],
        };
        walk(receipt, &locator, &doc.fields, out);
        if matches!(locator.owner, OwnerIdentity::SettingsDocument { .. })
            && let Some(projected) = resolved_value(receipt, &locator)
        {
            super::projection::scan(receipt, &locator, &doc.fields, projected, out);
        }
        for projected in receipt
            .graph()
            .documents
            .iter()
            .filter(|d| d.path == path && d.source.file_id == doc.file_id)
        {
            if projected.source.instance.is_empty() {
                super::projection::scan(receipt, &locator, &doc.fields, &projected.fields, out);
            } else {
                super::effective::document(receipt, &locator, projected, out);
            }
        }
    }
    Ok(())
}
fn walk(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    value: &Value,
    out: &mut Vec<LocatedReference>,
) {
    if super::projection::reserved(value) {
        out.push(super::projection::blocked(
            receipt,
            locator,
            value,
            "reserved_reference_syntax_in_original_source",
        ));
    } else if value.get("fileID").is_some() && value["fileID"].as_i64() != Some(0) {
        out.push(observation(receipt, locator, value));
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let mut child = locator.clone();
                child.field.push(FieldSegment::Key(key.clone()));
                walk(receipt, &child, value, out);
            }
        }
        Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                let mut child = locator.clone();
                child.field.push(FieldSegment::Index(
                    u32::try_from(index).expect("bounded source array"),
                ));
                walk(receipt, &child, value, out);
            }
        }
        _ => {}
    }
}
fn observation(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    raw: &Value,
) -> LocatedReference {
    let reference = serialized(raw, &locator.owner);
    let resolution = reference.as_ref().map_or_else(
        || unresolved("malformed_reference"),
        |r| resolve(receipt, locator, r),
    );
    LocatedReference {
        locator: locator.clone(),
        source_sha256: receipt.graph().source_hashes[&locator.source_path].clone(),
        reference,
        raw_reference: raw.clone(),
        resource_kind: resource_kind(locator),
        resolution,
    }
}
pub(super) fn serialized(raw: &Value, owner: &OwnerIdentity) -> Option<SerializedReference> {
    let local = match owner {
        OwnerIdentity::AssetDocument { asset_guid, .. }
        | OwnerIdentity::AssetMetadata { asset_guid, .. }
        | OwnerIdentity::ExpandedDocument { asset_guid, .. } => asset_guid.as_str(),
        OwnerIdentity::SettingsDocument { .. } => "",
    };
    let guid = raw.get("guid").map_or(Some(local), Value::as_str)?;
    if !guid.is_empty() && !super::policy::hex(guid, 32) {
        return None;
    }
    let reference_type = if let Some(value) = raw.get("type") {
        Some(i32::try_from(value.as_i64()?).ok()?)
    } else {
        None
    };
    Some(SerializedReference {
        guid: guid.into(),
        file_id: raw["fileID"].as_i64()?,
        reference_type,
    })
}
fn resolve(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    reference: &SerializedReference,
) -> ReferenceResolution {
    if let Some(value) = resolved_value(receipt, locator) {
        if super::projection::marker_count(value) > 1 {
            return unresolved("ambiguous_projected_reference");
        }
        if let Some(name) = value["$builtin"].as_str() {
            return ReferenceResolution::BuiltinMapping {
                provider_id: name.into(),
            };
        }
        if let Some(name) = value["$type"].as_str() {
            return ReferenceResolution::ManagedTypeMapping {
                type_name: name.into(),
            };
        }
        if let Some(id) = value["$ref"].as_str() {
            if value["guid"].as_str() != Some(reference.guid.as_str())
                || value["file_id"].as_i64() != Some(reference.file_id)
                || !super::resource::matches_reference(receipt, id, reference)
            {
                return unresolved("substituted_original_reference");
            }
            return super::resource::target(receipt, id, resource_kind(locator));
        }
        if value.get("$missing").is_some() {
            return unresolved("original_target_unavailable");
        }
        if serialized(value, &locator.owner).as_ref() != Some(reference) {
            return unresolved("substituted_original_reference");
        }
    } else if !super::resource::stripped_owner(receipt, &locator.owner) {
        return unresolved("projected_reference_unavailable");
    }
    let target = format!("{}:{}", reference.guid, reference.file_id);
    if receipt
        .graph()
        .documents
        .iter()
        .any(|d| d.source.key() == target)
    {
        return super::resource::target(receipt, &target, resource_kind(locator));
    }
    unresolved("original_target_unavailable")
}
fn unresolved(code: &str) -> ReferenceResolution {
    ReferenceResolution::Unresolved { code: code.into() }
}
fn resolved_value<'a>(
    receipt: &'a ContentReceipt,
    locator: &ReferenceLocator,
) -> Option<&'a Value> {
    let graph = receipt.graph();
    let mut value = match &locator.owner {
        OwnerIdentity::ExpandedDocument { .. } => return None,
        OwnerIdentity::SettingsDocument { document_index, .. } => graph
            .metadata_records
            .get(&locator.source_path)?
            .get("documents")?
            .get(*document_index as usize)?,
        OwnerIdentity::AssetMetadata { .. } => {
            let record = graph.metadata_records.get(&locator.source_path)?;
            // Environment metadata uses the settings envelope; asset metadata is direct.
            if record.as_object().is_some_and(|map| map.len() == 1)
                && record["documents"].is_object()
            {
                &record["documents"]
            } else {
                record
            }
        }
        OwnerIdentity::AssetDocument {
            asset_guid,
            file_id,
            ..
        } => {
            &graph
                .documents
                .iter()
                .find(|d| d.source.guid == *asset_guid && d.source.file_id == *file_id)?
                .fields
        }
    };
    for segment in &locator.field {
        value = match segment {
            FieldSegment::Key(key) => value.get(key)?,
            FieldSegment::Index(index) => value.get(*index as usize)?,
        };
    }
    Some(value)
}
pub(super) fn resource_kind(locator: &ReferenceLocator) -> ResourceKind {
    let key = locator
        .field
        .iter()
        .rev()
        .find_map(|s| match s {
            FieldSegment::Key(k) => Some(k.as_str()),
            FieldSegment::Index(_) => None,
        })
        .unwrap_or("");
    match key {
        "script" if matches!(locator.owner, OwnerIdentity::AssetMetadata { .. }) => {
            ResourceKind::ImporterScript
        }
        "m_Script" => ResourceKind::Script,
        "m_Shader" | "m_AlwaysIncludedShaders" => ResourceKind::Shader,
        "m_Material" if super::resource::physics_owner(&locator.owner) => {
            ResourceKind::PhysicsMaterial2D
        }
        "m_Material" | "m_Materials" | "m_SpritesDefaultMaterial" => ResourceKind::Material,
        "m_Font" => ResourceKind::Font,
        "m_Texture" | "m_MainTexture" | "m_SpotCookie" => ResourceKind::Texture,
        _ => ResourceKind::Unknown,
    }
}
