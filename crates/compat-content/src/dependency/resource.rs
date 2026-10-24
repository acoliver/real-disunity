use super::{OwnerIdentity, ReferenceResolution, ResourceKind, SerializedReference};
use crate::{AssetKind, ContentReceipt};

pub(super) fn target(
    receipt: &ContentReceipt,
    target: &str,
    expected: ResourceKind,
) -> ReferenceResolution {
    let compatible =
        match expected {
            ResourceKind::Unknown | ResourceKind::Object => available(receipt, target),
            ResourceKind::PhysicsMaterial2D => receipt.graph().documents.iter().any(|d| {
                d.source.key() == target && d.class_id == 62 && d.kind == "PhysicsMaterial2D"
            }),
            ResourceKind::Material => {
                kind(receipt, target) == Some(AssetKind::Material)
                    && receipt.graph().documents.iter().any(|d| {
                        d.source.key() == target && d.class_id == 21 && d.kind == "Material"
                    })
            }
            ResourceKind::Font => kind(receipt, target) == Some(AssetKind::Font),
            ResourceKind::Texture => kind(receipt, target) == Some(AssetKind::Texture),
            ResourceKind::Script | ResourceKind::ImporterScript => {
                kind(receipt, target) == Some(AssetKind::Script)
            }
            ResourceKind::Shader | ResourceKind::RenderSettings | ResourceKind::Input => false,
        };
    if compatible {
        ReferenceResolution::ResolvedTarget {
            target: target.into(),
        }
    } else {
        ReferenceResolution::Unresolved {
            code: "target_kind_mismatch_or_shader_provider_required".into(),
        }
    }
}
pub(super) fn physics_owner(owner: &OwnerIdentity) -> bool {
    let (OwnerIdentity::AssetDocument { class_id, kind, .. }
    | OwnerIdentity::ExpandedDocument { class_id, kind, .. }) = owner
    else {
        return false;
    };
    matches!(
        (*class_id, kind.as_str()),
        (50, "Rigidbody2D")
            | (61, "BoxCollider2D")
            | (58, "CircleCollider2D")
            | (60, "PolygonCollider2D")
            | (19_719_996, "TilemapCollider2D")
    )
}
pub(super) fn matches_reference(
    receipt: &ContentReceipt,
    target: &str,
    reference: &SerializedReference,
) -> bool {
    let original = format!("{}:{}", reference.guid, reference.file_id);
    target == original
        || receipt
            .graph()
            .aliases
            .get(&original)
            .is_some_and(|id| id == target)
        || receipt.graph().documents.iter().any(|d| {
            d.source.key() == target
                && d.source.guid == reference.guid
                && d.source.file_id == reference.file_id
        })
}
fn available(receipt: &ContentReceipt, target: &str) -> bool {
    if receipt
        .graph()
        .documents
        .iter()
        .any(|d| d.source.key() == target)
    {
        return true;
    }
    let Some((guid, file_id)) = target.split_once(':') else {
        return false;
    };
    kind(receipt, target).is_some_and(|kind| {
        !receipt
            .graph()
            .documents
            .iter()
            .any(|d| d.source.guid == guid)
            || (kind == AssetKind::Prefab && file_id == "100100000")
    })
}
fn kind(receipt: &ContentReceipt, target: &str) -> Option<AssetKind> {
    let (guid, _) = target.split_once(':')?;
    receipt
        .graph()
        .assets
        .iter()
        .find(|a| a.guid == guid)
        .map(|a| a.kind.clone())
}

pub(super) fn stripped_owner(receipt: &ContentReceipt, owner: &OwnerIdentity) -> bool {
    let OwnerIdentity::AssetDocument {
        asset_guid,
        file_id,
        ..
    } = owner
    else {
        return false;
    };
    let original = format!("{asset_guid}:{file_id}");
    // Stripped provenance has an expansion alias and no direct projected document.
    receipt.graph().aliases.contains_key(&original)
        && !receipt
            .graph()
            .documents
            .iter()
            .any(|d| d.source.key() == original)
}
