//! Decoded script fields as plan JSON. Values are written as a project scene's YAML gives
//! them, so the loader's field binder reads both alike: numbers and booleans as text, Unity
//! values and serializable classes as objects of their fields, and references as plan
//! entries.

use serde_json::{Map, Value as Json};
use worldrunner_unity_serialized::{
    object::PPtr,
    script_data::{REFERENCES, Value},
};

/// Writes decoded fields, resolving each non-null object reference with `resolve`. A
/// reference it cannot resolve, with the reason it gives, and every `[SerializeReference]`
/// value are written as `null` and listed in `differences`. The reference registry itself is
/// not written.
pub(crate) struct FieldWriter<'d, F> {
    resolve: F,
    differences: &'d mut Vec<String>,
}

impl<'d, F: FnMut(PPtr) -> Result<Json, String>> FieldWriter<'d, F> {
    pub(crate) fn new(resolve: F, differences: &'d mut Vec<String>) -> Self {
        Self {
            resolve,
            differences,
        }
    }

    /// The fields as a JSON object; `place` names the owner in differences.
    pub(crate) fn fields(&mut self, place: &str, fields: &[(String, Value)]) -> Json {
        let mut out = Map::new();
        for (name, value) in fields {
            if name != REFERENCES {
                let json = self.value(&format!("{place}.{name}"), value);
                out.insert(name.clone(), json);
            }
        }
        Json::Object(out)
    }

    fn value(&mut self, path: &str, value: &Value) -> Json {
        match value {
            Value::Bool(flag) => Json::String(if *flag { "1" } else { "0" }.to_owned()),
            Value::Int(number) => Json::String(number.to_string()),
            Value::UInt(number) => Json::String(number.to_string()),
            Value::Float(number) => Json::String(float_text(*number)),
            Value::String(text) => Json::String(text.clone()),
            Value::Array(items) => Json::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| self.value(&format!("{path}[{i}]"), item))
                    .collect(),
            ),
            Value::Struct(fields) => self.fields(path, fields),
            Value::Object(pptr) => self.reference(path, *pptr),
            Value::Reference(_) => {
                self.differences
                    .push(format!("{path}: [SerializeReference] values are not built"));
                Json::Null
            }
        }
    }

    fn reference(&mut self, path: &str, pptr: PPtr) -> Json {
        if pptr.is_null() {
            return Json::Null;
        }
        match (self.resolve)(pptr) {
            Ok(json) => json,
            Err(reason) => {
                self.differences.push(format!("{path}: {reason}"));
                Json::Null
            }
        }
    }
}

/// Why a reference to an object the plan does not hold is left out.
pub(crate) fn not_held(pptr: PPtr) -> String {
    format!(
        "references object {} of file {}, which the plan does not hold",
        pptr.path_id, pptr.file_id
    )
}

/// A float as the loader parses it; Rust writes infinities as `inf`, which .NET does not read.
fn float_text(number: f64) -> String {
    if number.is_infinite() {
        if number > 0.0 {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_owned()
    } else {
        number.to_string()
    }
}
