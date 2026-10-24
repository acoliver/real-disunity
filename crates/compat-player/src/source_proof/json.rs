use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

struct Bounded(usize);

impl<'de> DeserializeSeed<'de> for Bounded {
    type Value = Value;

    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<Value, D::Error> {
        if self.0 > 32 {
            return Err(D::Error::custom("inspection JSON depth exceeded"));
        }
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Bounded {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded unique-key JSON")
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("invalid number"))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_unit<E: Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Bounded(self.0 + 1))? {
            if values.len() >= 100_000 {
                return Err(A::Error::custom("inspection census exceeded"));
            }
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.len() >= 100_000 || values.contains_key(&key) {
                return Err(A::Error::custom("duplicate or oversized inspection map"));
            }
            values.insert(key, map.next_value_seed(Bounded(self.0 + 1))?);
        }
        Ok(Value::Object(values))
    }
}

pub(super) fn parse(bytes: &[u8]) -> Result<Value, String> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Bounded(0).deserialize(&mut de).map_err(|e| e.to_string())?;
    de.end().map_err(|e| e.to_string())?;
    Ok(value)
}

#[cfg(test)]
#[path = "../../tests/source_proof_helpers/json.rs"]
mod tests;
