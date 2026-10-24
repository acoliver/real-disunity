//! Scoped numeric projection for native evidence. Ordinary request parsing is unchanged.
use std::fmt;

use serde::Deserializer;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value, value::RawValue};

use super::RESPONSE_LIMIT;
use crate::HostError;

const MAX_NODES: usize = 262_144;
const MAX_DEPTH: usize = 64;
const MAX_ITEMS: usize = 65_536;

fn invalid(message: impl fmt::Display) -> HostError {
    HostError::Request(format!("exact native evidence: {message}"))
}

pub(super) fn project(text: &str) -> Result<Value, HostError> {
    if text.len() > RESPONSE_LIMIT {
        return Err(HostError::ResponseBudget(RESPONSE_LIMIT));
    }
    let raw: &RawValue = serde_json::from_str(text).map_err(invalid)?;
    decode(raw, 0, &mut 0)
}

pub(super) fn response(text: &str) -> Result<Value, HostError> {
    let mut value = project(text)?;
    let map = value
        .as_object_mut()
        .ok_or_else(|| invalid("expected envelope"))?;
    if map.len() != 3
        || map.get("version").and_then(Value::as_u64) != Some(1)
        || map.get("ok").and_then(Value::as_bool) != Some(true)
    {
        return Err(invalid("invalid success envelope"));
    }
    map.remove("data").ok_or_else(|| invalid("missing data"))
}

fn decode(raw: &RawValue, depth: usize, nodes: &mut usize) -> Result<Value, HostError> {
    if depth > MAX_DEPTH || *nodes >= MAX_NODES {
        return Err(invalid("depth or node budget exceeded"));
    }
    *nodes += 1;
    let text = raw.get();
    match text.as_bytes()[0] {
        b'{' | b'[' => {
            let mut de = serde_json::Deserializer::from_str(text);
            de.deserialize_any(Collection { depth, nodes })
                .map_err(invalid)
        }
        b'"' => serde_json::from_str(text)
            .map(Value::String)
            .map_err(invalid),
        b'n' => Ok(Value::Null),
        b't' => Ok(Value::Bool(true)),
        b'f' => Ok(Value::Bool(false)),
        _ => number(text),
    }
}

fn number(text: &str) -> Result<Value, HostError> {
    if !text.contains(['.', 'e', 'E']) && text != "-0" {
        if let Ok(n) = text.parse::<u64>() {
            return Ok(Value::from(n));
        }
        if let Ok(n) = text.parse::<i64>() {
            return Ok(Value::from(n));
        }
        return Err(invalid("integer outside 64-bit range"));
    }
    text.parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map(Value::Number)
        .ok_or_else(|| invalid("nonfinite or invalid binary64"))
}

struct Collection<'a> {
    depth: usize,
    nodes: &'a mut usize,
}

impl<'de> Visitor<'de> for Collection<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON collection")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if values.len() >= MAX_ITEMS || values.contains_key(&key) {
                return Err(serde::de::Error::custom(
                    "duplicate key or object budget exceeded",
                ));
            }
            let raw = access.next_value::<&RawValue>()?;
            let value =
                decode(raw, self.depth + 1, self.nodes).map_err(serde::de::Error::custom)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(raw) = access.next_element::<&RawValue>()? {
            if values.len() >= MAX_ITEMS {
                return Err(serde::de::Error::custom("array budget exceeded"));
            }
            values.push(decode(raw, self.depth + 1, self.nodes).map_err(serde::de::Error::custom)?);
        }
        Ok(Value::Array(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_scalars_and_duplicate_rejection() {
        let v =
            project(r#"[0.36050745844841003,-0.0,5e-324,18446744073709551615,"\u263a"]"#).unwrap();
        for (index, text) in ["0.36050745844841003", "-0.0", "5e-324"].iter().enumerate() {
            assert_eq!(
                v[index].as_f64().unwrap().to_bits(),
                text.parse::<f64>().unwrap().to_bits()
            );
        }
        assert_eq!(v[3].as_u64(), Some(u64::MAX));
        assert_eq!(v[4], "☺");
        for text in [
            r#"{"a":{"x":1,"\u0078":2}}"#,
            "1e999",
            "18446744073709551616",
        ] {
            assert!(project(text).is_err(), "{text}");
        }
        assert!(project(&format!("{}0{}", "[".repeat(66), "]".repeat(66))).is_err());
        assert!(project(&format!("[{}0]", "0,".repeat(MAX_ITEMS))).is_err());
    }
    #[test]
    fn aggregate_nodes_are_bounded_across_small_collections() {
        let array = format!("[{}0]", "0,".repeat(32_767));
        let entries: Vec<_> = (0..9).map(|n| format!("\"{n}\":{array}")).collect();
        let text = format!("{{{}}}", entries.join(","));
        let error = project(&text).unwrap_err().to_string();
        assert!(error.contains("node budget"), "{error}");
        assert!(project(&" ".repeat(RESPONSE_LIMIT + 1)).is_err());
    }
}
