//! Decoded script fields as plan JSON. Values are written as a project scene's YAML gives
//! them, so the loader's field binder reads both alike: numbers and booleans as text, Unity
//! values and serializable classes as objects of their fields, and references as plan
//! entries.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json, json};
use worldrunner_unity_serialized::{
    object::PPtr,
    script_data::{REFERENCES, Value},
};

/// Writes decoded fields, resolving each non-null object reference with `resolve`. A
/// reference it cannot resolve, with the reason it gives, is written as `null` and listed in
/// `differences`. A `[SerializeReference]` value is written where it is referenced, as
/// `{"$managed": {"class", "assembly", "fields"}}` from the object's reference registry,
/// which is not written itself; a value that is null (ID -2) is `null`, and an unknown,
/// missing or self-containing one is `null` with a difference.
pub(crate) struct FieldWriter<'d, F> {
    resolve: F,
    differences: &'d mut Vec<String>,
    /// The registry of the object being written, by ID.
    registry: BTreeMap<i64, Registered>,
    /// The IDs being written, to stop a value that contains itself.
    active: Vec<i64>,
}

/// A `[SerializeReference]` value of an object's registry.
struct Registered {
    class: String,
    assembly: String,
    data: Vec<(String, Value)>,
}

/// The value assigned to `[SerializeReference]` fields that hold nothing.
const NULL_REFERENCE: i64 = -2;

impl<'d, F: FnMut(PPtr) -> Result<Json, String>> FieldWriter<'d, F> {
    pub(crate) fn new(resolve: F, differences: &'d mut Vec<String>) -> Self {
        Self {
            resolve,
            differences,
            registry: BTreeMap::new(),
            active: Vec::new(),
        }
    }

    /// The fields as a JSON object; `place` names the owner in differences.
    pub(crate) fn fields(&mut self, place: &str, fields: &[(String, Value)]) -> Json {
        let mut out = Map::new();
        if let Some((_, Value::Array(entries))) = fields.iter().find(|(name, _)| name == REFERENCES)
        {
            self.registry = entries.iter().filter_map(registered).collect();
        }
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
            Value::Reference(id) => self.managed(path, *id),
        }
    }

    fn managed(&mut self, path: &str, id: i64) -> Json {
        if id == NULL_REFERENCE {
            return Json::Null;
        }
        if self.active.contains(&id) {
            self.differences.push(format!(
                "{path}: [SerializeReference] value {id} contains itself"
            ));
            return Json::Null;
        }
        let Some(value) = self.registry.remove(&id) else {
            self.differences.push(format!(
                "{path}: [SerializeReference] value {id} is not in the object's reference registry"
            ));
            return Json::Null;
        };
        self.active.push(id);
        let fields = self.fields(path, &value.data);
        self.active.pop();
        let managed = json!({"class": value.class, "assembly": value.assembly, "fields": fields});
        self.registry.insert(id, value);
        json!({"$managed": managed})
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

/// A registry entry, or `None` for one with no value (a negative ID).
fn registered(entry: &Value) -> Option<(i64, Registered)> {
    let Value::Struct(parts) = entry else {
        return None;
    };
    let get = |name: &str| parts.iter().find(|(n, _)| n == name).map(|(_, v)| v);
    let (Some(Value::Int(id)), Some(Value::String(class)), Some(Value::String(space))) =
        (get("rid"), get("class"), get("namespace"))
    else {
        return None;
    };
    let (Some(Value::String(assembly)), Some(Value::Struct(data))) = (get("assembly"), get("data"))
    else {
        return None;
    };
    if *id < 0 || class.is_empty() {
        return None;
    }
    let class = if space.is_empty() {
        class.clone()
    } else {
        format!("{space}.{class}")
    };
    Some((
        *id,
        Registered {
            class,
            assembly: assembly.clone(),
            data: data.clone(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, class: &str, data: Vec<(String, Value)>) -> Value {
        Value::Struct(vec![
            ("rid".to_owned(), Value::Int(id)),
            ("class".to_owned(), Value::String(class.to_owned())),
            ("namespace".to_owned(), Value::String("Game".to_owned())),
            (
                "assembly".to_owned(),
                Value::String("Game.Runtime".to_owned()),
            ),
            ("data".to_owned(), Value::Struct(data)),
        ])
    }

    fn write(fields: &[(String, Value)]) -> (Json, Vec<String>) {
        let mut differences = Vec::new();
        let json = FieldWriter::new(|_| Ok(Json::Null), &mut differences).fields("Owner", fields);
        (json, differences)
    }

    fn owner(list: Vec<Value>, registry: Vec<Value>) -> Vec<(String, Value)> {
        vec![
            ("items".to_owned(), Value::Array(list)),
            (REFERENCES.to_owned(), Value::Array(registry)),
        ]
    }

    #[test]
    fn writes_a_referenced_value_where_it_is_referenced() {
        let data = vec![("size".to_owned(), Value::Int(4))];
        let fields = owner(
            vec![Value::Reference(7), Value::Reference(NULL_REFERENCE)],
            vec![entry(7, "Quality", data)],
        );
        let (json, differences) = write(&fields);
        assert_eq!(
            json,
            json!({"items": [
                {"$managed": {"class": "Game.Quality", "assembly": "Game.Runtime", "fields": {"size": "4"}}},
                null,
            ]})
        );
        assert!(differences.is_empty(), "{differences:?}");
    }

    #[test]
    fn writes_a_value_referenced_twice_each_time() {
        let fields = owner(
            vec![Value::Reference(1), Value::Reference(1)],
            vec![entry(1, "Quality", Vec::new())],
        );
        let (json, differences) = write(&fields);
        assert_eq!(json["items"][0], json["items"][1]);
        assert!(json["items"][1]["$managed"].is_object());
        assert!(differences.is_empty(), "{differences:?}");
    }

    #[test]
    fn leaves_out_an_unknown_value_and_one_that_contains_itself() {
        let looping = vec![("next".to_owned(), Value::Reference(3))];
        let fields = owner(
            vec![Value::Reference(9), Value::Reference(3)],
            vec![entry(3, "Loop", looping)],
        );
        let (json, differences) = write(&fields);
        assert_eq!(json["items"][0], Json::Null);
        assert_eq!(json["items"][1]["$managed"]["fields"]["next"], Json::Null);
        assert_eq!(
            differences,
            [
                "Owner.items[0]: [SerializeReference] value 9 is not in the object's reference registry",
                "Owner.items[1].next: [SerializeReference] value 3 contains itself",
            ]
        );
    }
}
