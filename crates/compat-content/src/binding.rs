use crate::{
    catalog::Project,
    model::{Diagnostic, ImportProfile, ScriptBinding},
    prefab::{Record, diagnostic},
};
use serde_json::Value;

pub(crate) fn script(
    record: &Record,
    fields: &Value,
    project: &Project,
    profile: &ImportProfile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ScriptBinding> {
    let guid = record.doc.fields["m_Script"]["guid"].as_str()?;
    let file_id = record.doc.fields["m_Script"]["fileID"]
        .as_i64()
        .unwrap_or(0);
    let key = format!("{guid}:{file_id}");
    let (path, type_name) = project
        .scripts
        .get(guid)
        .cloned()
        .or_else(|| {
            profile
                .script_types
                .get(&key)
                .or_else(|| profile.script_types.get(guid))
                .map(|name| (String::new(), name.clone()))
        })
        .or_else(|| {
            fields["m_EditorClassIdentifier"]
                .as_str()?
                .split_once("::")
                .filter(|(assembly, _)| *assembly != "Assembly-CSharp")
                .map(|(_, name)| (String::new(), name.to_owned()))
        })?;
    let mut values = fields.as_object().cloned().unwrap_or_default();
    for key in RESERVED_FIELDS {
        values.remove(*key);
    }
    migrate_fields(record, &mut values, profile, diagnostics);
    exclude_fields(record, &mut values, profile, diagnostics);
    Some(ScriptBinding {
        guid: guid.into(),
        path,
        type_name,
        fields: Value::Object(values),
    })
}

fn migrate_fields(
    record: &Record,
    values: &mut serde_json::Map<String, Value>,
    profile: &ImportProfile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let guid = record.doc.fields["m_Script"]["guid"].as_str().unwrap_or("");
    if let Some(aliases) = profile.field_aliases.get(guid) {
        for (old, new) in aliases {
            if let Some(value) = values.remove(old) {
                if values.contains_key(new) {
                    diagnostics.push(diagnostic(
                        record,
                        "info",
                        "field_alias_superseded",
                        Some(old),
                        "explicit current field value takes precedence over aliased stale field",
                    ));
                } else {
                    values.insert(new.clone(), value);
                    diagnostics.push(diagnostic(
                        record,
                        "info",
                        "source_field_alias",
                        Some(old),
                        &format!("source profile maps stale serialized field {old} to {new}"),
                    ));
                }
            }
        }
    }
}

fn exclude_fields(
    record: &Record,
    values: &mut serde_json::Map<String, Value>,
    profile: &ImportProfile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let guid = record.doc.fields["m_Script"]["guid"].as_str().unwrap_or("");
    let Some(exclusions) = profile.field_exclusions.get(guid) else {
        return;
    };
    for (field, reason) in exclusions {
        if values.contains_key(field) && !reason.trim().is_empty() {
            values.remove(field);
            diagnostics.push(diagnostic(
                record,
                "info",
                "source_field_excluded",
                Some(field),
                reason,
            ));
        }
    }
}

const RESERVED_FIELDS: &[&str] = &[
    "m_ObjectHideFlags",
    "m_CorrespondingSourceObject",
    "m_PrefabParentObject",
    "m_PrefabInstance",
    "m_PrefabInternal",
    "m_PrefabAsset",
    "m_GameObject",
    "m_Enabled",
    "m_EditorHideFlags",
    "m_Script",
    "m_Name",
    "m_EditorClassIdentifier",
    "serializedVersion",
];
