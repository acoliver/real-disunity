//! Decoded script fields as plan JSON. Values are written as a project scene's YAML gives
//! them, so the loader's field binder reads both alike: numbers and booleans as text, Unity
//! values and serializable classes as objects of their fields, and references as plan
//! entries.

use serde_json::{Map, Value as Json, json};
use worldrunner_unity_serialized::{
    object::PPtr,
    script_data::{REFERENCES, Value},
};

use super::{PlayerPlanner, Target};

impl PlayerPlanner {
    /// The script fields as a JSON object. The reference registry is not planned: a
    /// `[SerializeReference]` field is planned as `null`, with a difference.
    pub(super) fn fields(&mut self, place: &str, fields: &[(String, Value)]) -> Json {
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
        let target = (pptr.file_id == 0)
            .then(|| self.index.get(&pptr.path_id).copied())
            .flatten();
        match target {
            Some(Target::Object(i)) => json!({ "$object": i }),
            Some(Target::Transform(i)) => json!({ "$transform": i }),
            Some(Target::Component(i, j)) => json!({ "$component": [i, j] }),
            None => {
                self.differences.push(format!(
                    "{path}: references object {} of file {}, which the plan does not hold",
                    pptr.path_id, pptr.file_id
                ));
                Json::Null
            }
        }
    }
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
