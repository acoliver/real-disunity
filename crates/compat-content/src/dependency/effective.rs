use super::{
    FieldSegment, LocatedReference, OwnerIdentity, ReferenceLocator, ReferenceResolution,
    SerializedReference, observe, projection, resource,
};
use crate::{ContentReceipt, ImportedDocument};
use serde_json::{Value, json};

pub(super) fn document(
    receipt: &ContentReceipt,
    original: &ReferenceLocator,
    document: &ImportedDocument,
    out: &mut Vec<LocatedReference>,
) {
    let OwnerIdentity::AssetDocument { document_index, .. } = original.owner else {
        return;
    };
    let locator = ReferenceLocator {
        source_path: document.path.clone(),
        owner: OwnerIdentity::ExpandedDocument {
            asset_guid: document.source.guid.clone(),
            document_index,
            class_id: document.class_id,
            file_id: document.source.file_id,
            kind: document.kind.clone(),
            instance: document.source.instance.clone(),
        },
        field: vec![],
    };
    walk(receipt, &locator, &document.fields, out);
}
fn walk(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    value: &Value,
    out: &mut Vec<LocatedReference>,
) {
    if projection::reserved(value)
        || (value.get("fileID").is_some() && value["fileID"].as_i64() != Some(0))
    {
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
                    u32::try_from(index).expect("bounded expanded array"),
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
    value: &Value,
) -> LocatedReference {
    let raw = if projection::reserved(value) {
        json!({"guid": value["guid"], "fileID": value["file_id"]})
    } else {
        value.clone()
    };
    let reference = observe::serialized(&raw, &locator.owner);
    let resolution = reference.as_ref().map_or_else(
        || unresolved("malformed_effective_reference"),
        |reference| resolve(receipt, locator, value, reference),
    );
    LocatedReference {
        locator: locator.clone(),
        source_sha256: receipt.graph().source_hashes[&locator.source_path].clone(),
        reference,
        raw_reference: value.clone(),
        resource_kind: observe::resource_kind(locator),
        resolution,
    }
}
fn resolve(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    value: &Value,
    reference: &SerializedReference,
) -> ReferenceResolution {
    if projection::marker_count(value) > 1 || reference.file_id == 0 {
        return unresolved("ambiguous_or_null_effective_reference");
    }
    if let Some(target) = value["$ref"].as_str() {
        if !resource::matches_reference(receipt, target, reference) {
            return unresolved("effective_target_identity_mismatch");
        }
        return resource::target(receipt, target, observe::resource_kind(locator));
    }
    if let Some(provider_id) = value["$builtin"].as_str().filter(|s| !s.is_empty()) {
        return ReferenceResolution::BuiltinMapping {
            provider_id: provider_id.into(),
        };
    }
    if let Some(type_name) = value["$type"].as_str().filter(|s| !s.is_empty()) {
        return ReferenceResolution::ManagedTypeMapping {
            type_name: type_name.into(),
        };
    }
    if projection::reserved(value) {
        return unresolved("unavailable_or_malformed_effective_reference");
    }
    // Prefab provenance fields intentionally retain raw serialized references.
    let key = format!("{}:{}", reference.guid, reference.file_id);
    let target = receipt.graph().aliases.get(&key).unwrap_or(&key);
    resource::target(receipt, target, observe::resource_kind(locator))
}
fn unresolved(code: &str) -> ReferenceResolution {
    ReferenceResolution::Unresolved { code: code.into() }
}
