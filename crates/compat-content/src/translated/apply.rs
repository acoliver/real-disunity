use super::{
    ComponentProjection, ExecutionRecord, OriginalSelectionDecision, SelectionDisposition,
    SelectionOperation, SelectionPolicy,
};
use crate::{ContentReceipt, FieldSegment, ImportedScene, LocatedReference, OwnerIdentity};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn select(
    receipt: &ContentReceipt,
    policy: &SelectionPolicy,
) -> Result<(ImportedScene, Vec<OriginalSelectionDecision>), String> {
    if policy.schema_version != 1
        || policy.profile_id.trim().is_empty()
        || policy.operations.is_empty()
        || policy.required_capabilities.is_empty()
        || policy.original_closure_sha256 != receipt.closure_sha256()
    {
        return Err("selection version/profile/current closure precondition failed".into());
    }
    let mut graph = receipt.graph().clone();
    let mut ids = BTreeSet::new();
    for op in &policy.operations {
        if op.id().trim().is_empty() || !ids.insert(op.id()) {
            return Err("empty/duplicate operation ID".into());
        }
        validate_effect(op)?;
        super::scope::validate(receipt, op)?;
        validate_opaque(receipt, op)?;
        if op
            .effect()
            .capability
            .as_ref()
            .is_some_and(|c| c.id == crate::CapabilityId::CanonicalKeyboardSnapshot)
            && !policy.keyboard_only
        {
            return Err("input exclusion requires the keyboard-only assembly/API-use proof".into());
        }
        let path = source_path(receipt.graph(), op.record())?;
        if receipt.graph().source_hashes.get(&path).map(String::as_str) != Some(op.source_sha256())
        {
            return Err(format!(
                "operation {} source byte precondition failed",
                op.id()
            ));
        }
        apply(&mut graph, op)?;
    }
    Ok((graph, ledger(receipt, policy)))
}
fn ledger(receipt: &ContentReceipt, policy: &SelectionPolicy) -> Vec<OriginalSelectionDecision> {
    receipt
        .dependency_references()
        .iter()
        .map(|observation| {
            let operations: Vec<_> = policy
                .operations
                .iter()
                .filter(|op| covers(observation, op))
                .collect();
            let disposition = if operations
                .iter()
                .any(|op| matches!(op, SelectionOperation::ExcludeRecord { .. }))
            {
                SelectionDisposition::ExcludedFromExecution
            } else if operations.is_empty() {
                SelectionDisposition::Retained
            } else {
                SelectionDisposition::Replaced
            };
            OriginalSelectionDecision {
                observation: observation.clone(),
                disposition,
                operation_ids: operations.iter().map(|op| op.id().into()).collect(),
            }
        })
        .collect()
}
fn validate_opaque(receipt: &ContentReceipt, op: &SelectionOperation) -> Result<(), String> {
    if op.effect().capability.is_some() {
        return Ok(());
    }
    let ExecutionRecord::Metadata { path } = op.record() else {
        return Err("opaque selection requires exact settings record".into());
    };
    if path != "ProjectSettings/URPProjectSettings.asset"
        || op.effect().behavior != "opaque-unconsumed"
    {
        return Err("unqualified opaque record selection".into());
    }
    let text = std::str::from_utf8(receipt.sources().get(path).ok_or("opaque source absent")?)
        .map_err(|e| e.to_string())?;
    let docs = crate::yaml::parse_unity_yaml(text, 2)?;
    if docs.len() != 1
        || docs[0].class_id != 114
        || docs[0].file_id != 1
        || docs[0].kind != "MonoBehaviour"
        || docs[0].fields["m_Script"]
            != serde_json::json!({"fileID":11_500_000,"guid":"247994e1f5a72c2419c26a37e9334c01","type":3})
    {
        return Err("opaque URP settings identity precondition failed".into());
    }
    Ok(())
}
fn validate_effect(op: &SelectionOperation) -> Result<(), String> {
    let effect = op.effect();
    if effect.behavior.trim().is_empty()
        || effect.reason.trim().is_empty()
        || effect.lost_effects.is_empty()
        || effect.lost_effects.iter().any(|s| s.trim().is_empty())
    {
        return Err("selection requires named behavior, reason and explicit lost effects".into());
    }
    if effect.capability.is_none()
        && !matches!(
            op,
            SelectionOperation::ExcludeRecord {
                record: ExecutionRecord::Metadata { .. },
                ..
            }
        )
    {
        return Err("execution translation requires an exact typed capability".into());
    }
    Ok(())
}
pub(super) fn source_path(
    graph: &ImportedScene,
    record: &ExecutionRecord,
) -> Result<String, String> {
    match record {
        ExecutionRecord::Metadata { path } => Ok(path.clone()),
        ExecutionRecord::Document { source } | ExecutionRecord::Component { source, .. } => graph
            .documents
            .iter()
            .find(|d| d.source == *source)
            .map(|d| d.path.clone())
            .ok_or_else(|| "selected record does not exist".into()),
    }
}
fn record_mut<'a>(
    graph: &'a mut ImportedScene,
    record: &ExecutionRecord,
) -> Result<&'a mut Value, String> {
    match record {
        ExecutionRecord::Document { source } => graph
            .documents
            .iter_mut()
            .find(|d| d.source == *source)
            .map(|d| &mut d.fields),
        ExecutionRecord::Metadata { path } => graph.metadata_records.get_mut(path),
        ExecutionRecord::Component { source, projection } => {
            let c = graph
                .components
                .iter_mut()
                .find(|c| c.source == *source)
                .ok_or("selected component absent")?;
            match projection {
                ComponentProjection::Fields => Some(&mut c.fields),
                ComponentProjection::NativeFields => Some(&mut c.native_fields),
                ComponentProjection::ScriptFields => c.script.as_mut().map(|s| &mut s.fields),
            }
        }
    }
    .ok_or_else(|| "selected record absent".into())
}
fn apply(graph: &mut ImportedScene, op: &SelectionOperation) -> Result<(), String> {
    match op {
        SelectionOperation::ReplaceField {
            record,
            field,
            expected,
            replacement,
            ..
        } => {
            if field.is_empty() || field.len() > 96 {
                return Err("invalid exact field path".into());
            }
            let mut value = record_mut(graph, record)?;
            for segment in field {
                value = match segment {
                    FieldSegment::Key(k) => value.get_mut(k),
                    FieldSegment::Index(i) => value.get_mut(*i as usize),
                }
                .ok_or("selected field does not exist")?;
            }
            if value != expected {
                return Err(format!(
                    "operation {} old-value precondition failed",
                    op.id()
                ));
            }
            *value = replacement.clone();
        }
        SelectionOperation::ExcludeRecord {
            record,
            expected_sha256,
            ..
        } => {
            if crate::dependency::identity(record_mut(graph, record)?) != *expected_sha256 {
                return Err("excluded record exact-value precondition failed".into());
            }
            match record {
                ExecutionRecord::Metadata { path } => {
                    graph.metadata_records.remove(path);
                }
                ExecutionRecord::Document { source } => {
                    graph.documents.retain(|d| d.source != *source);
                }
                ExecutionRecord::Component { source, .. } => {
                    graph.components.retain(|c| c.source != *source);
                }
            }
        }
    }
    Ok(())
}
fn covers(observation: &LocatedReference, op: &SelectionOperation) -> bool {
    let locator = &observation.locator;
    let mut prefix = Vec::new();
    let matches = match (op.record(), &locator.owner) {
        (
            ExecutionRecord::Metadata { path },
            OwnerIdentity::SettingsDocument { document_index, .. },
        ) => {
            prefix = vec![
                FieldSegment::Key("documents".into()),
                FieldSegment::Index(*document_index),
            ];
            path == &locator.source_path
        }
        (ExecutionRecord::Metadata { path }, OwnerIdentity::AssetMetadata { .. }) => {
            path == &locator.source_path
        }
        (
            ExecutionRecord::Document { source },
            OwnerIdentity::AssetDocument {
                asset_guid,
                file_id,
                ..
            },
        ) => source.guid == *asset_guid && source.file_id == *file_id && source.instance.is_empty(),
        (
            ExecutionRecord::Document { source },
            OwnerIdentity::ExpandedDocument {
                asset_guid,
                file_id,
                instance,
                ..
            },
        ) => {
            source.guid == *asset_guid && source.file_id == *file_id && source.instance == *instance
        }
        _ => false,
    };
    prefix.extend(locator.field.clone());
    matches
        && match op {
            SelectionOperation::ReplaceField { field, .. } => prefix.starts_with(field),
            SelectionOperation::ExcludeRecord { .. } => true,
        }
}
