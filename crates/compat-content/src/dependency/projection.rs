use super::{FieldSegment, LocatedReference, ReferenceLocator, ReferenceResolution, ResourceKind};
use crate::ContentReceipt;
use serde_json::Value;

pub(super) fn reserved(value: &Value) -> bool {
    marker_count(value) != 0
}
pub(super) fn marker_count(value: &Value) -> usize {
    ["$ref", "$builtin", "$type", "$missing"]
        .iter()
        .filter(|key| value.get(**key).is_some())
        .count()
}
pub(super) fn blocked(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    value: &Value,
    code: &str,
) -> LocatedReference {
    LocatedReference {
        locator: locator.clone(),
        source_sha256: receipt.graph().source_hashes[&locator.source_path].clone(),
        reference: None,
        raw_reference: value.clone(),
        resource_kind: ResourceKind::Unknown,
        resolution: ReferenceResolution::Unresolved { code: code.into() },
    }
}
/// Projection markers must originate at a retained serialized-reference site.
/// This also inspects objectless records and fields unused by native conversion.
pub(super) fn scan(
    receipt: &ContentReceipt,
    locator: &ReferenceLocator,
    original: &Value,
    projected: &Value,
    out: &mut Vec<LocatedReference>,
) {
    if marker_count(projected) > 1
        || (reserved(projected)
            && original["fileID"].as_i64().is_none_or(|id| id == 0)
            && !reserved(original))
    {
        out.push(blocked(
            receipt,
            locator,
            projected,
            "unclassified_projected_reference",
        ));
    }
    match projected {
        Value::Object(map) => {
            for (key, value) in map {
                let mut child = locator.clone();
                child.field.push(FieldSegment::Key(key.clone()));
                scan(receipt, &child, &original[key], value, out);
            }
        }
        Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                let mut child = locator.clone();
                child.field.push(FieldSegment::Index(
                    u32::try_from(index).expect("bounded source array"),
                ));
                scan(receipt, &child, &original[index], value, out);
            }
        }
        _ => {}
    }
}
