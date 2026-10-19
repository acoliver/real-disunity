//! Strict, generic, versioned corpus semantic capability graph.
//!
//! The graph is data-only and dataset-neutral: it pins exact source selectors and optional
//! runtime output recipes to nonempty root-to-leaf class ancestries. It declares no source or save
//! fact — no path, handle, count, save identifier, digest, or population field is part of the
//! schema shape, and any such field arriving in JSON is rejected as unknown. A later registry must
//! match each recipe's exact selector spelling or fail closed.

use crate::validation::{first_unsorted_unique, is_identifier, is_uri};
use crate::version::{
    CAPABILITY_GRAPH_SCHEMA, CAPABILITY_GRAPH_VERSION, VersionedDocumentError, parse_versioned,
    to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use thiserror::Error;

/// The exact selector one recipe pins.
///
/// A selector names the source schema URI, a normalized source kind URI, and optionally one exact
/// source state. A missing state selects every state of that schema and kind; a present state
/// selects only that exact state. No wildcard is accepted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSelector {
    pub source_schema_uri: String,
    pub source_kind_uri: String,
    pub source_state: Option<String>,
}

impl SourceSelector {
    /// Returns the total ordering key over the selector fields.
    ///
    /// The missing state sorts before every present state, so a general selector of one schema and
    /// kind precedes its exact state selectors.
    fn key(&self) -> (&str, &str, Option<&str>) {
        (
            self.source_schema_uri.as_str(),
            self.source_kind_uri.as_str(),
            self.source_state.as_deref(),
        )
    }

    fn validate(&self) -> Result<(), CapabilityGraphError> {
        if !is_uri(&self.source_schema_uri) {
            return Err(invalid_field(
                "selector.source_schema_uri",
                "schema URI",
                &self.source_schema_uri,
            ));
        }
        if !is_uri(&self.source_kind_uri) {
            return Err(invalid_field(
                "selector.source_kind_uri",
                "normalized source kind URI",
                &self.source_kind_uri,
            ));
        }
        if let Some(state) = &self.source_state
            && !is_identifier(state)
        {
            return Err(invalid_field(
                "selector.source_state",
                "nonempty source state when present",
                state,
            ));
        }
        Ok(())
    }
}

/// One role-to-binding assignment declared by an output recipe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleBindingAssignment {
    pub role: String,
    pub binding_id: String,
}

impl RoleBindingAssignment {
    fn validate(&self) -> Result<(), CapabilityGraphError> {
        if !is_uri(&self.role) {
            return Err(invalid_field(
                "output.bindings.role",
                "role URI",
                &self.role,
            ));
        }
        if !is_identifier(&self.binding_id) {
            return Err(invalid_field(
                "output.bindings.binding_id",
                "nonempty binding identifier",
                &self.binding_id,
            ));
        }
        Ok(())
    }
}

/// Optional runtime output a selected source maps to.
///
/// The recipe names the output schema and identifier domain, then assigns each role exactly one
/// binding identifier in strictly ascending duplicate-free role order. The list may be empty; a
/// recipe may declare an output shape with no bound roles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeOutputRecipe {
    pub output_schema_uri: String,
    pub id_domain_uri: String,
    pub bindings: Vec<RoleBindingAssignment>,
}

impl RuntimeOutputRecipe {
    /// Finds an assignment by its role.
    ///
    /// Relies on the strictly sorted role contract established by validation.
    #[must_use]
    pub fn binding(&self, role: &str) -> Option<&RoleBindingAssignment> {
        self.bindings
            .binary_search_by_key(&role, |assignment| assignment.role.as_str())
            .ok()
            .map(|index| &self.bindings[index])
    }

    fn validate(&self) -> Result<(), CapabilityGraphError> {
        if !is_uri(&self.output_schema_uri) {
            return Err(invalid_field(
                "output.output_schema_uri",
                "output schema URI",
                &self.output_schema_uri,
            ));
        }
        if !is_uri(&self.id_domain_uri) {
            return Err(invalid_field(
                "output.id_domain_uri",
                "identifier domain URI",
                &self.id_domain_uri,
            ));
        }
        let roles: Vec<String> = self
            .bindings
            .iter()
            .map(|assignment| assignment.role.clone())
            .collect();
        if let Some(index) = first_unsorted_unique(&roles) {
            return Err(invalid_field(
                "output.bindings",
                "strictly sorted unique roles",
                &self.bindings[index].role,
            ));
        }
        for assignment in &self.bindings {
            assignment.validate()?;
        }
        Ok(())
    }
}

/// One capability-map recipe of the graph.
///
/// A recipe pins one exact selector to one nonempty root-to-leaf class ancestry and, optionally, to
/// one runtime output recipe. It declares no source or save observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRecipe {
    pub selector: SourceSelector,
    pub class_ancestry: Vec<String>,
    pub output: Option<RuntimeOutputRecipe>,
}

impl SemanticRecipe {
    fn validate(&self) -> Result<(), CapabilityGraphError> {
        self.selector.validate()?;
        validate_ancestry(&self.class_ancestry)?;
        if let Some(output) = &self.output {
            output.validate()?;
        }
        Ok(())
    }
}

fn validate_ancestry(ancestry: &[String]) -> Result<(), CapabilityGraphError> {
    if ancestry.is_empty() {
        return Err(invalid_field(
            "recipe.class_ancestry",
            "nonempty root-to-leaf class ancestry",
            "empty",
        ));
    }
    let mut seen = BTreeSet::new();
    for class in ancestry {
        if !is_identifier(class) {
            return Err(invalid_field(
                "recipe.class_ancestry",
                "nonempty class segment",
                class,
            ));
        }
        if !seen.insert(class.as_str()) {
            return Err(invalid_field(
                "recipe.class_ancestry",
                "root-to-leaf class ancestry with no repeated class",
                class,
            ));
        }
    }
    Ok(())
}

/// Strict, versioned corpus semantic capability graph document.
///
/// Recipes are declared in strictly ascending unique selector order: schema URI, kind URI, and
/// optional state. A general selector must not coexist with exact state selectors of the same schema
/// and kind, because resolution would be ambiguous.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityGraphDocument {
    pub schema: String,
    pub schema_version: u32,
    pub graph_id: String,
    pub recipes: Vec<SemanticRecipe>,
}

impl CapabilityGraphDocument {
    /// Parses and validates a capability graph document.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or graph validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CapabilityGraphError> {
        let graph: Self =
            parse_versioned(bytes, CAPABILITY_GRAPH_SCHEMA, CAPABILITY_GRAPH_VERSION)?;
        graph.validate()?;
        Ok(graph)
    }

    /// Serializes this graph as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the graph is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, CapabilityGraphError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this graph as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the graph is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, CapabilityGraphError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Finds the unambiguous recipe for this source selector, if any.
    ///
    /// An exact state selector takes precedence. When no exact state selector exists, a selector
    /// without a state applies to every state of the same schema and kind. Validation rejects a
    /// graph that authors both forms for one schema and kind, so resolution cannot be ambiguous.
    #[must_use]
    pub fn recipe_for(
        &self,
        source_schema_uri: &str,
        source_kind_uri: &str,
        source_state: Option<&str>,
    ) -> Option<&SemanticRecipe> {
        self.recipe_for_exact(source_schema_uri, source_kind_uri, source_state)
            .or_else(|| {
                source_state
                    .and_then(|_| self.recipe_for_exact(source_schema_uri, source_kind_uri, None))
            })
    }

    fn recipe_for_exact(
        &self,
        source_schema_uri: &str,
        source_kind_uri: &str,
        source_state: Option<&str>,
    ) -> Option<&SemanticRecipe> {
        let key = (source_schema_uri, source_kind_uri, source_state);
        self.recipes
            .binary_search_by(|recipe| recipe.selector.key().cmp(&key))
            .ok()
            .map(|index| &self.recipes[index])
    }

    /// Validates identity, ordering, uniqueness, selectors, ancestry, and output bindings.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed capability-graph violation.
    pub fn validate(&self) -> Result<(), CapabilityGraphError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            CAPABILITY_GRAPH_SCHEMA,
            CAPABILITY_GRAPH_VERSION,
        )?;
        if !is_identifier(&self.graph_id) {
            return Err(invalid_field(
                "graph_id",
                "nonempty identifier",
                &self.graph_id,
            ));
        }
        validate_recipes(&self.recipes)
    }
}

fn validate_recipes(recipes: &[SemanticRecipe]) -> Result<(), CapabilityGraphError> {
    if recipes.is_empty() {
        return Err(invalid_field(
            "recipes",
            "at least one semantic recipe",
            "empty",
        ));
    }
    let mut previous_key: Option<(&str, &str, Option<&str>)> = None;
    for recipe in recipes {
        recipe.validate()?;
        let key = recipe.selector.key();
        if let Some(previous) = previous_key {
            match previous.cmp(&key) {
                Ordering::Less => {}
                Ordering::Equal => {
                    return Err(invalid_field(
                        "recipes",
                        "strictly sorted unique recipes",
                        "duplicate selector",
                    ));
                }
                Ordering::Greater => {
                    return Err(invalid_field(
                        "recipes",
                        "strictly sorted unique recipes",
                        "out of order",
                    ));
                }
            }
        }
        previous_key = Some(key);
    }
    reject_ambiguous_selectors(recipes)
}

fn reject_ambiguous_selectors(recipes: &[SemanticRecipe]) -> Result<(), CapabilityGraphError> {
    let mut index = 0;
    while index < recipes.len() {
        let mut end = index + 1;
        while end < recipes.len()
            && recipes[end].selector.source_schema_uri == recipes[index].selector.source_schema_uri
            && recipes[end].selector.source_kind_uri == recipes[index].selector.source_kind_uri
        {
            end += 1;
        }
        let general_present = recipes[index..end]
            .iter()
            .any(|recipe| recipe.selector.source_state.is_none());
        let state_present = recipes[index..end]
            .iter()
            .any(|recipe| recipe.selector.source_state.is_some());
        if general_present && state_present {
            return Err(invalid_field(
                "selector",
                "exact selector; one rule per source schema and kind",
                "general and state selectors overlap",
            ));
        }
        index = end;
    }
    Ok(())
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> CapabilityGraphError {
    CapabilityGraphError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Capability-graph decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum CapabilityGraphError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("capability-graph field {field} mismatch: expected {expected}, observed {observed}")]
    FieldMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn selector() -> SourceSelector {
        SourceSelector {
            source_schema_uri: "urn:example:schema:opaque-record".to_owned(),
            source_kind_uri: "urn:example:kind:clutter-family".to_owned(),
            source_state: None,
        }
    }

    fn recipe() -> SemanticRecipe {
        SemanticRecipe {
            selector: selector(),
            class_ancestry: vec![
                "Environment".to_owned(),
                "Flora".to_owned(),
                "Tree".to_owned(),
            ],
            output: Some(RuntimeOutputRecipe {
                output_schema_uri: "urn:example:schema:instanced-clutter".to_owned(),
                id_domain_uri: "urn:example:domain:clutter-identities".to_owned(),
                bindings: vec![
                    RoleBindingAssignment {
                        role: "urn:example:role:material".to_owned(),
                        binding_id: "resource-material".to_owned(),
                    },
                    RoleBindingAssignment {
                        role: "urn:example:role:mesh".to_owned(),
                        binding_id: "resource-mesh".to_owned(),
                    },
                ],
            }),
        }
    }

    fn subsequent_recipe() -> SemanticRecipe {
        SemanticRecipe {
            selector: SourceSelector {
                source_schema_uri: "urn:example:schema:opaque-record".to_owned(),
                source_kind_uri: "urn:example:kind:rock-family".to_owned(),
                source_state: None,
            },
            class_ancestry: vec![
                "Environment".to_owned(),
                "Terrain".to_owned(),
                "Rock".to_owned(),
            ],
            output: None,
        }
    }

    fn graph() -> CapabilityGraphDocument {
        CapabilityGraphDocument {
            schema: CAPABILITY_GRAPH_SCHEMA.to_owned(),
            schema_version: CAPABILITY_GRAPH_VERSION,
            graph_id: "arbitrary-corpus-capability-graph".to_owned(),
            recipes: vec![recipe(), subsequent_recipe()],
        }
    }

    fn key_tree_contains(value: &Value, needle: &str) -> bool {
        match value {
            Value::Object(map) => {
                map.keys().any(|key| key.contains(needle))
                    || map.values().any(|child| key_tree_contains(child, needle))
            }
            _ => false,
        }
    }

    #[test]
    fn graph_round_trip_is_deterministic_canonical_and_pretty() {
        let authored = graph();
        let canonical = authored.to_json_bytes().unwrap();
        assert_eq!(authored.to_json_bytes().unwrap(), canonical);
        assert_eq!(
            CapabilityGraphDocument::from_json(&canonical).unwrap(),
            authored
        );
        let reparsed =
            CapabilityGraphDocument::from_json(&authored.to_json_pretty().unwrap()).unwrap();
        assert_eq!(reparsed, authored);
    }

    #[test]
    fn recipe_and_output_binding_lookups_resolve_exact_selectors() {
        let authored = graph();
        let found = authored
            .recipe_for(
                "urn:example:schema:opaque-record",
                "urn:example:kind:clutter-family",
                None,
            )
            .unwrap();
        let output = found.output.as_ref().unwrap();
        assert_eq!(
            output
                .binding("urn:example:role:mesh")
                .map(|binding| binding.binding_id.as_str()),
            Some("resource-mesh")
        );
        assert!(
            authored
                .recipe_for(
                    "urn:example:schema:opaque-record",
                    "urn:example:kind:rock-family",
                    None,
                )
                .unwrap()
                .output
                .is_none()
        );
        assert!(
            authored
                .recipe_for(
                    "urn:example:schema:opaque-record",
                    "urn:example:kind:rock-family",
                    Some("flowering"),
                )
                .unwrap()
                .output
                .is_none(),
            "a selector without a state applies to every state of its schema and kind"
        );
        assert!(
            authored
                .recipe_for(
                    "urn:example:schema:opaque-record",
                    "urn:example:kind:unknown-family",
                    Some("flowering"),
                )
                .is_none()
        );
    }

    #[test]
    fn canonical_json_declares_no_source_or_save_facts() {
        let value: Value = serde_json::from_slice(&graph().to_json_bytes().unwrap()).unwrap();
        for concept in [
            "path",
            "handle",
            "count",
            "save",
            "hash",
            "digest",
            "population",
            "bytes",
        ] {
            assert!(
                !key_tree_contains(&value, concept),
                "capability graph must not declare {concept}"
            );
        }
    }

    #[test]
    fn empty_recipes_and_empty_graph_ids_are_rejected() {
        let mut empty = graph();
        empty.recipes.clear();
        assert!(matches!(
            empty.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipes",
                ..
            })
        ));

        let mut blank_id = graph();
        blank_id.graph_id = "  ".to_owned();
        assert!(matches!(
            blank_id.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "graph_id",
                ..
            })
        ));
    }

    #[test]
    fn unsorted_and_duplicate_recipes_are_rejected() {
        let mut unsorted = graph();
        unsorted.recipes.reverse();
        assert!(matches!(
            unsorted.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipes",
                ..
            })
        ));

        let mut duplicated = graph();
        duplicated.recipes[1] = duplicated.recipes[0].clone();
        assert!(matches!(
            duplicated.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipes",
                ..
            })
        ));
    }

    #[test]
    fn duplicate_selectors_are_rejected() {
        // An identical selector makes the second recipe a duplicate of the first.
        let mut duplicated = graph();
        duplicated.recipes[0].selector = duplicated.recipes[1].selector.clone();
        assert!(matches!(
            duplicated.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipes",
                ..
            })
        ));
    }

    #[test]
    fn ambiguous_general_and_state_selectors_are_rejected() {
        let mut ambiguous = graph();
        ambiguous.recipes[0].selector.source_state = Some("flowering".to_owned());
        ambiguous.recipes.insert(
            0,
            SemanticRecipe {
                selector: SourceSelector {
                    source_schema_uri: "urn:example:schema:opaque-record".to_owned(),
                    source_kind_uri: "urn:example:kind:clutter-family".to_owned(),
                    source_state: None,
                },
                class_ancestry: vec![
                    "Environment".to_owned(),
                    "Flora".to_owned(),
                    "Grass".to_owned(),
                ],
                output: None,
            },
        );
        // Recipes are general clutter, flowering clutter, rock — ascending by selector.
        assert!(matches!(
            ambiguous.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "selector",
                ..
            })
        ));
    }

    #[test]
    fn exact_state_selectors_may_coexist_for_one_schema_and_kind() {
        let mut distinct = graph();
        distinct.recipes[0].selector.source_state = Some("dry".to_owned());
        distinct.recipes.insert(
            1,
            SemanticRecipe {
                selector: SourceSelector {
                    source_schema_uri: "urn:example:schema:opaque-record".to_owned(),
                    source_kind_uri: "urn:example:kind:clutter-family".to_owned(),
                    source_state: Some("wet".to_owned()),
                },
                class_ancestry: vec![
                    "Environment".to_owned(),
                    "Flora".to_owned(),
                    "Shrub".to_owned(),
                ],
                output: None,
            },
        );
        // Recipes are dry clutter, wet clutter, rock — ascending by selector.
        assert!(distinct.validate().is_ok());
    }

    #[test]
    fn malformed_selectors_are_rejected() {
        let mut schemeless = graph();
        schemeless.recipes[0].selector.source_schema_uri = "missing-scheme-separator".to_owned();
        assert!(matches!(
            schemeless.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "selector.source_schema_uri",
                ..
            })
        ));

        let mut numeric_scheme = graph();
        numeric_scheme.recipes[0].selector.source_kind_uri = "1invalid-kind".to_owned();
        assert!(matches!(
            numeric_scheme.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "selector.source_kind_uri",
                ..
            })
        ));

        let mut blank_state = graph();
        blank_state.recipes[0].selector.source_state = Some(" ".to_owned());
        assert!(matches!(
            blank_state.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "selector.source_state",
                ..
            })
        ));
    }

    #[test]
    fn empty_ancestry_is_rejected() {
        let mut empty = graph();
        empty.recipes[0].class_ancestry.clear();
        assert!(matches!(
            empty.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipe.class_ancestry",
                ..
            })
        ));

        let mut blank_segment = graph();
        blank_segment.recipes[0].class_ancestry[0] = "not an identifier".to_owned();
        assert!(matches!(
            blank_segment.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipe.class_ancestry",
                ..
            })
        ));

        let mut repeated = graph();
        repeated.recipes[0].class_ancestry[1] = "Environment".to_owned();
        assert!(matches!(
            repeated.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "recipe.class_ancestry",
                ..
            })
        ));
    }

    #[test]
    fn unsorted_and_duplicate_output_bindings_are_rejected() {
        let mut unsorted = graph();
        unsorted.recipes[0]
            .output
            .as_mut()
            .unwrap()
            .bindings
            .reverse();
        assert!(matches!(
            unsorted.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "output.bindings",
                ..
            })
        ));

        let mut duplicated = graph();
        let output = duplicated.recipes[0].output.as_mut().unwrap();
        output.bindings[1].role = output.bindings[0].role.clone();
        assert!(matches!(
            duplicated.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "output.bindings",
                ..
            })
        ));
    }

    #[test]
    fn malformed_output_uris_and_bindings_are_rejected() {
        let mut schemeless = graph();
        schemeless.recipes[0]
            .output
            .as_mut()
            .unwrap()
            .output_schema_uri = "output".to_owned();
        assert!(matches!(
            schemeless.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "output.output_schema_uri",
                ..
            })
        ));

        let mut empty_domain = graph();
        empty_domain.recipes[0]
            .output
            .as_mut()
            .unwrap()
            .id_domain_uri
            .clear();
        assert!(matches!(
            empty_domain.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "output.id_domain_uri",
                ..
            })
        ));

        let mut bad_role = graph();
        bad_role.recipes[0].output.as_mut().unwrap().bindings[0].role = "no-scheme".to_owned();
        assert!(matches!(
            bad_role.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "output.bindings.role",
                ..
            })
        ));

        let mut bad_binding_id = graph();
        bad_binding_id.recipes[0].output.as_mut().unwrap().bindings[0].binding_id =
            "not an identifier".to_owned();
        assert!(matches!(
            bad_binding_id.validate(),
            Err(CapabilityGraphError::FieldMismatch {
                field: "output.bindings.binding_id",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let text = String::from_utf8(graph().to_json_bytes().unwrap())
            .unwrap()
            .replacen('{', "{\"unexpected\":1,", 1);
        assert!(matches!(
            CapabilityGraphDocument::from_json(text.as_bytes()),
            Err(CapabilityGraphError::Document(
                VersionedDocumentError::Json(_)
            ))
        ));

        let text = String::from_utf8(graph().to_json_bytes().unwrap())
            .unwrap()
            .replace(
                &format!("\"schema_version\":{CAPABILITY_GRAPH_VERSION}"),
                &format!("\"schema_version\":{}", CAPABILITY_GRAPH_VERSION + 1),
            );
        assert!(matches!(
            CapabilityGraphDocument::from_json(text.as_bytes()),
            Err(CapabilityGraphError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }

    #[test]
    fn json_source_and_save_fact_fields_are_rejected_as_unknown() {
        let mut top_level: Value =
            serde_json::from_slice(&graph().to_json_bytes().unwrap()).unwrap();
        top_level
            .as_object_mut()
            .unwrap()
            .insert("save_id".to_owned(), json!("arbitrary-save"));
        assert!(matches!(
            CapabilityGraphDocument::from_json(&serde_json::to_vec(&top_level).unwrap()),
            Err(CapabilityGraphError::Document(
                VersionedDocumentError::Json(_)
            ))
        ));

        let mut nested: Value = serde_json::from_slice(&graph().to_json_bytes().unwrap()).unwrap();
        nested
            .as_object_mut()
            .unwrap()
            .get_mut("recipes")
            .unwrap()
            .as_array_mut()
            .unwrap()[0]
            .as_object_mut()
            .unwrap()
            .insert("population".to_owned(), json!(7));
        assert!(matches!(
            CapabilityGraphDocument::from_json(&serde_json::to_vec(&nested).unwrap()),
            Err(CapabilityGraphError::Document(
                VersionedDocumentError::Json(_)
            ))
        ));
    }
}
