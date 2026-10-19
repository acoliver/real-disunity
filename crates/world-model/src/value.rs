//! Universal semantic value algebra shared by decoded saves and world documents.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Deterministic, float-free value algebra for semantic payloads.
///
/// The representation is deliberately universal: any decoded or authored
/// semantic payload can be expressed without referencing a particular save
/// format, corpus, or schema family. Determinism comes from the absence of
/// floats and from ordered maps, which makes canonical hashing well defined.
///
/// The serde representation is adjacently tagged (`{"type": ..., "value": ...}`)
/// and rejects unknown fields so same-version payloads cannot silently gain
/// unmodelled meaning when deserialized by downstream users.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SemanticValue {
    /// Absence of a value.
    Null,
    /// Boolean scalar.
    Bool(bool),
    /// Signed 64-bit integer scalar.
    Signed(i64),
    /// Unsigned 64-bit integer scalar.
    Unsigned(u64),
    /// UTF-8 text scalar.
    Text(String),
    /// Ordered sequence of nested values.
    Sequence(Vec<SemanticValue>),
    /// Keyed map of nested values with deterministically ordered keys.
    Map(SemanticMap),
    /// Reference to a [`crate::ResourceBinding`] by binding identifier.
    BindingRef(String),
}

impl SemanticValue {
    /// Visits every nested resource-binding reference in deterministic encounter order.
    pub fn visit_binding_refs(&self, visitor: &mut dyn FnMut(&str)) {
        match self {
            Self::Sequence(items) => {
                for item in items {
                    item.visit_binding_refs(visitor);
                }
            }
            Self::Map(map) => map.visit_binding_refs(visitor),
            Self::BindingRef(binding_id) => visitor(binding_id),
            Self::Null | Self::Bool(_) | Self::Signed(_) | Self::Unsigned(_) | Self::Text(_) => {}
        }
    }
}

/// Ordered string-keyed map of [`SemanticValue`] entries.
///
/// Wrapping [`BTreeMap`] keeps key iteration deterministic for canonical
/// hashing. Key validity (nonempty, free of control characters) is enforced by
/// validation, not construction, so unvalidated data can still be represented
/// before it is checked against explicit limits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SemanticMap(BTreeMap<String, SemanticValue>);

impl SemanticMap {
    /// Creates a map from an ordered map of entries without validation.
    pub fn new(entries: BTreeMap<String, SemanticValue>) -> Self {
        Self(entries)
    }

    /// Creates an empty map.
    pub fn empty() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns the entry for `key`, if present.
    pub fn get(&self, key: &str) -> Option<&SemanticValue> {
        self.0.get(key)
    }

    /// Iterates the entries in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SemanticValue)> {
        self.0.iter()
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Visits every resource-binding reference below this map in deterministic key order.
    pub fn visit_binding_refs(&self, visitor: &mut dyn FnMut(&str)) {
        for value in self.0.values() {
            value.visit_binding_refs(visitor);
        }
    }

    /// Consumes the map and returns the underlying entries.
    pub fn into_inner(self) -> BTreeMap<String, SemanticValue> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_binding_references_visit_in_deterministic_encounter_order() {
        let nested = SemanticMap::new(BTreeMap::from([
            (
                "a".to_owned(),
                SemanticValue::Sequence(vec![
                    SemanticValue::BindingRef("first".to_owned()),
                    SemanticValue::Map(SemanticMap::new(BTreeMap::from([(
                        "z".to_owned(),
                        SemanticValue::BindingRef("second".to_owned()),
                    )]))),
                ]),
            ),
            (
                "b".to_owned(),
                SemanticValue::BindingRef("third".to_owned()),
            ),
        ]));
        let mut observed = Vec::new();
        nested.visit_binding_refs(&mut |binding_id| observed.push(binding_id.to_owned()));
        assert_eq!(observed, ["first", "second", "third"]);
    }
}
