use crate::validation::{first_unsorted_unique, is_identifier, is_logical_path, is_uri};
use crate::version::{
    CORPUS_BINDING_SCHEMA, CORPUS_BINDING_VERSION, VersionedDocumentError, parse_versioned,
    to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One strict corpus-binding declaration authored into a capability document.
///
/// A record binds one stable identifier to one full corpus artifact through a capability URI and
/// carries an optional structural element URI inside that content. It never declares a
/// digest, a byte length, or a filesystem-path form: content identity is resolved later
/// from the exact corpus-manifest artifact entry the capability document names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusBindingRecord {
    pub binding_id: String,
    pub capability_uri: String,
    pub artifact_path: String,
    pub element_uri: Option<String>,
}

/// Strict versioned generic corpus-binding capability document.
///
/// The document externally declares its bindings in strictly ascending unique `binding_id`
/// order. Every binding must be nonempty and name only manifest-internal artifact paths; the
/// payload contract is fixed by the document URI, and no unknown field survives.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusBindingDocument {
    pub schema: String,
    pub schema_version: u32,
    pub bindings: Vec<CorpusBindingRecord>,
}

impl CorpusBindingDocument {
    /// Parses and validates a strict corpus-binding capability document.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or document validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CorpusBindingError> {
        let document: Self = parse_versioned(bytes, CORPUS_BINDING_SCHEMA, CORPUS_BINDING_VERSION)?;
        document.validate()?;
        Ok(document)
    }

    /// Serializes this document as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the document is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, CorpusBindingError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this document as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the document is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, CorpusBindingError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Validates identity, ordering, uniqueness, URIs, and artifact paths.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed corpus-binding-document violation.
    pub fn validate(&self) -> Result<(), CorpusBindingError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            CORPUS_BINDING_SCHEMA,
            CORPUS_BINDING_VERSION,
        )?;
        let identifiers: Vec<String> = self
            .bindings
            .iter()
            .map(|record| record.binding_id.clone())
            .collect();
        if self.bindings.is_empty() {
            return Err(invalid_field(
                "bindings",
                "at least one binding declaration",
                "empty",
            ));
        }
        if let Some(index) = first_unsorted_unique(&identifiers) {
            return Err(invalid_field(
                "bindings.binding_id",
                "strictly sorted unique identifiers",
                &self.bindings[index].binding_id,
            ));
        }
        for record in &self.bindings {
            validate_record(record)?;
        }
        Ok(())
    }
}

fn validate_record(record: &CorpusBindingRecord) -> Result<(), CorpusBindingError> {
    if !is_identifier(&record.binding_id) {
        return Err(invalid_field(
            "bindings.binding_id",
            "nonempty identifier",
            &record.binding_id,
        ));
    }
    if !is_uri(&record.capability_uri) {
        return Err(invalid_field(
            "bindings.capability_uri",
            "capability URI",
            &record.capability_uri,
        ));
    }
    if !is_logical_path(&record.artifact_path) {
        return Err(invalid_field(
            "bindings.artifact_path",
            "normalized relative logical path",
            &record.artifact_path,
        ));
    }
    if let Some(element_uri) = &record.element_uri
        && !is_uri(element_uri)
    {
        return Err(invalid_field(
            "bindings.element_uri",
            "structural URI when present",
            element_uri,
        ));
    }
    Ok(())
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> CorpusBindingError {
    CorpusBindingError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Corpus-binding capability-document decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum CorpusBindingError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("corpus-binding field {field} mismatch: expected {expected}, observed {observed}")]
    FieldMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn document() -> CorpusBindingDocument {
        CorpusBindingDocument {
            schema: CORPUS_BINDING_SCHEMA.to_owned(),
            schema_version: CORPUS_BINDING_VERSION,
            bindings: vec![
                CorpusBindingRecord {
                    binding_id: "resource-alpha".to_owned(),
                    capability_uri: "urn:example:capability:alpha".to_owned(),
                    artifact_path: "bundles/alpha.bin".to_owned(),
                    element_uri: Some("urn:example:element:alpha".to_owned()),
                },
                CorpusBindingRecord {
                    binding_id: "resource-beta".to_owned(),
                    capability_uri: "urn:example:capability:beta".to_owned(),
                    artifact_path: "bundles/nested/beta.bin".to_owned(),
                    element_uri: Some("urn:example:element:beta-leaves".to_owned()),
                },
            ],
        }
    }

    #[test]
    fn corpus_binding_round_trip_is_deterministic_canonical_and_pretty() {
        let authored = document();
        let canonical = authored.to_json_bytes().unwrap();
        assert_eq!(authored.to_json_bytes().unwrap(), canonical);
        assert_eq!(
            CorpusBindingDocument::from_json(&canonical).unwrap(),
            authored
        );
        let reparsed =
            CorpusBindingDocument::from_json(&authored.to_json_pretty().unwrap()).unwrap();
        assert_eq!(reparsed, authored);
    }

    #[test]
    fn full_artifact_binding_may_omit_structural_element_uri() {
        let mut full_artifact = document();
        full_artifact.bindings[0].element_uri = None;
        let encoded = full_artifact.to_json_bytes().unwrap();
        assert_eq!(
            CorpusBindingDocument::from_json(&encoded).unwrap(),
            full_artifact
        );
    }

    #[test]
    fn duplicate_unsorted_and_empty_bindings_fail_closed() {
        let mut empty = document();
        empty.bindings.clear();
        assert!(matches!(
            empty.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings",
                ..
            })
        ));

        let mut unsorted = document();
        unsorted.bindings.reverse();
        assert!(matches!(
            unsorted.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings.binding_id",
                ..
            })
        ));

        let mut duplicated = document();
        duplicated.bindings[1].binding_id = "resource-alpha".to_owned();
        assert!(matches!(
            duplicated.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings.binding_id",
                ..
            })
        ));
    }

    #[test]
    fn unsafe_paths_and_malformed_uris_and_identifiers_fail_closed() {
        let mut escape = document();
        escape.bindings[0].artifact_path = "../outside.bin".to_owned();
        assert!(matches!(
            escape.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings.artifact_path",
                ..
            })
        ));

        let mut schemeless = document();
        schemeless.bindings[0].capability_uri = "missing-scheme-separator".to_owned();
        assert!(matches!(
            schemeless.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings.capability_uri",
                ..
            })
        ));

        let mut empty_id = document();
        empty_id.bindings[0].binding_id = "  ".to_owned();
        assert!(matches!(
            empty_id.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings.binding_id",
                ..
            })
        ));
    }

    #[test]
    fn absolute_host_and_body_path_forms_are_rejected() {
        let mut rescans = document();
        rescans.bindings[1].artifact_path = "C:/bundles/beta.bin".to_owned();
        assert!(matches!(
            rescans.validate(),
            Err(CorpusBindingError::FieldMismatch {
                field: "bindings.artifact_path",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let text = String::from_utf8(document().to_json_bytes().unwrap())
            .unwrap()
            .replacen('{', "{\"unexpected\":1,", 1);
        assert!(matches!(
            CorpusBindingDocument::from_json(text.as_bytes()),
            Err(CorpusBindingError::Document(VersionedDocumentError::Json(
                _
            )))
        ));

        let text = String::from_utf8(document().to_json_bytes().unwrap())
            .unwrap()
            .replace(
                &format!("\"schema_version\":{CORPUS_BINDING_VERSION}"),
                &format!("\"schema_version\":{}", CORPUS_BINDING_VERSION + 1),
            );
        assert!(matches!(
            CorpusBindingDocument::from_json(text.as_bytes()),
            Err(CorpusBindingError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }

    #[test]
    fn arbitrary_renamed_document_round_trips_with_varying_bound_entries() {
        let mut renamed = document();
        renamed.bindings[0].binding_id = "renamed-resource".to_owned();
        renamed.bindings[0].artifact_path = "arbitrary/renamed.bin".to_owned();
        renamed.bindings.push(CorpusBindingRecord {
            binding_id: "zeta-renamed".to_owned(),
            capability_uri: "urn:example:capability:zeta-renamed".to_owned(),
            artifact_path: "arbitrary/zeta-renamed.bin".to_owned(),
            element_uri: Some("urn:example:element:zeta-renamed".to_owned()),
        });
        let value: Value = serde_json::from_slice(&renamed.to_json_bytes().unwrap()).unwrap();
        assert!(value.get("bindings").is_some());
        assert!(
            CorpusBindingDocument::from_json(&renamed.to_json_bytes().unwrap())
                .unwrap()
                .validate()
                .is_ok()
        );
    }
}
