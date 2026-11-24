//! Reads the script layouts Disunity.Engine's `ScriptLayouts` writes from a player's
//! assemblies: `{"scripts": [key], "types": {key: {"value_type": bool, "fields": [...]}}}`,
//! where each field is `{"name": text, "type": type}` and a type is one of
//! `{"primitive": name}`, `{"object": class}`, `{"builtin": name}`, `{"composite": key}`,
//! `{"array": type}` or `{"reference": class}`.

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;
use worldrunner_unity_serialized::{
    script_builtin::Builtin,
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
};

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("the script layouts are not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{key}.{field}: {reason}")]
    Field {
        key: String,
        field: String,
        reason: String,
    },
}

#[derive(Deserialize)]
struct File {
    types: BTreeMap<String, RawType>,
}

#[derive(Deserialize)]
struct RawType {
    value_type: bool,
    fields: Vec<RawField>,
}

#[derive(Deserialize)]
struct RawField {
    name: String,
    #[serde(rename = "type")]
    ty: RawFieldType,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawFieldType {
    Primitive(String),
    Object(String),
    Builtin(String),
    Composite(String),
    Array(Box<RawFieldType>),
    Reference(String),
}

/// Parses script layouts. Composite keys are resolved to indices, so every composite a
/// field names must be listed.
///
/// # Errors
/// Fails on invalid JSON, an unknown primitive or builtin, or a composite that is not listed.
pub fn parse_script_layouts(json: &str) -> Result<ScriptLayouts, LayoutError> {
    let file: File = serde_json::from_str(json)?;
    let index: BTreeMap<&str, usize> = file
        .types
        .keys()
        .enumerate()
        .map(|(i, key)| (key.as_str(), i))
        .collect();
    let mut types = Vec::with_capacity(file.types.len());
    for (key, raw) in &file.types {
        let mut fields = Vec::with_capacity(raw.fields.len());
        for field in &raw.fields {
            let ty = field_type(&field.ty, &index).map_err(|reason| LayoutError::Field {
                key: key.clone(),
                field: field.name.clone(),
                reason,
            })?;
            fields.push(ScriptField {
                name: field.name.clone(),
                ty,
            });
        }
        types.push(ScriptType {
            key: key.clone(),
            value_type: raw.value_type,
            fields,
        });
    }
    Ok(ScriptLayouts::new(types))
}

fn field_type(raw: &RawFieldType, index: &BTreeMap<&str, usize>) -> Result<FieldType, String> {
    Ok(match raw {
        RawFieldType::Primitive(name) => FieldType::Primitive(
            Primitive::from_name(name).ok_or_else(|| format!("unknown primitive {name}"))?,
        ),
        RawFieldType::Object(class) => FieldType::Object(class.clone()),
        RawFieldType::Builtin(name) => FieldType::Builtin(
            Builtin::from_name(name).ok_or_else(|| format!("unknown builtin {name}"))?,
        ),
        RawFieldType::Composite(key) => FieldType::Composite(
            *index
                .get(key.as_str())
                .ok_or_else(|| format!("composite {key} is not listed"))?,
        ),
        RawFieldType::Array(element) => FieldType::Array(Box::new(field_type(element, index)?)),
        RawFieldType::Reference(class) => FieldType::Reference(class.clone()),
    })
}
