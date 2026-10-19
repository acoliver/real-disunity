use crate::validation::{
    HashValidity, first_unsorted_unique, is_identifier, is_logical_path, is_uri, sha256_validity,
};
use crate::version::{
    CORPUS_MANIFEST_SCHEMA, CORPUS_MANIFEST_VERSION, VersionedDocumentError, parse_versioned,
    to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

/// A content-addressed reusable corpus artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusArtifact {
    pub logical_path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// An extensible capability document or recipe referenced by URI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusCapabilityDocument {
    pub document_id: String,
    pub document_uri: String,
    pub logical_path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Independently authored and independently identified reusable-asset corpus.
///
/// The manifest owns its own identity and enumerates only content-addressed artifacts and
/// capability documents. It references no save bundle, lock, or import pipeline state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub schema: String,
    pub schema_version: u32,
    pub manifest_id: String,
    pub corpus_id: String,
    pub artifacts: Vec<CorpusArtifact>,
    pub documents: Vec<CorpusCapabilityDocument>,
}

impl CorpusManifest {
    /// Parses and validates a corpus manifest.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or manifest validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CorpusManifestError> {
        let manifest: Self =
            parse_versioned(bytes, CORPUS_MANIFEST_SCHEMA, CORPUS_MANIFEST_VERSION)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serializes this manifest as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, CorpusManifestError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this manifest as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, CorpusManifestError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Finds an artifact by its normalized logical path.
    #[must_use]
    pub fn artifact(&self, logical_path: &str) -> Option<&CorpusArtifact> {
        self.artifacts
            .binary_search_by_key(&logical_path, |artifact| artifact.logical_path.as_str())
            .ok()
            .map(|index| &self.artifacts[index])
    }

    /// Finds a capability document by its stable document identifier.
    #[must_use]
    pub fn document(&self, document_id: &str) -> Option<&CorpusCapabilityDocument> {
        self.documents
            .binary_search_by_key(&document_id, |document| document.document_id.as_str())
            .ok()
            .map(|index| &self.documents[index])
    }

    /// Validates identity, ordering, uniqueness, hashes, and document URIs.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed corpus-manifest violation.
    pub fn validate(&self) -> Result<(), CorpusManifestError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            CORPUS_MANIFEST_SCHEMA,
            CORPUS_MANIFEST_VERSION,
        )?;
        if !is_identifier(&self.manifest_id) {
            return Err(invalid_field(
                "manifest_id",
                "nonempty identifier",
                &self.manifest_id,
            ));
        }
        if !is_identifier(&self.corpus_id) {
            return Err(invalid_field(
                "corpus_id",
                "nonempty identifier",
                &self.corpus_id,
            ));
        }
        validate_artifacts(&self.artifacts)?;
        validate_documents(&self.documents)?;
        if self.artifacts.is_empty() && self.documents.is_empty() {
            return Err(invalid_field(
                "corpus",
                "at least one artifact or capability document",
                "empty",
            ));
        }
        validate_path_uniqueness(self)?;
        Ok(())
    }
}

fn validate_artifacts(artifacts: &[CorpusArtifact]) -> Result<(), CorpusManifestError> {
    let paths: Vec<String> = artifacts
        .iter()
        .map(|artifact| artifact.logical_path.clone())
        .collect();
    if let Some(index) = first_unsorted_unique(&paths) {
        return Err(invalid_field(
            "artifacts",
            "strictly sorted unique logical paths",
            &artifacts[index].logical_path,
        ));
    }
    for artifact in artifacts {
        if !is_logical_path(&artifact.logical_path) {
            return Err(invalid_field(
                "artifacts.logical_path",
                "normalized relative logical path",
                &artifact.logical_path,
            ));
        }
        validate_hash("artifacts.sha256", &artifact.sha256, &artifact.logical_path)?;
    }
    Ok(())
}

fn validate_documents(documents: &[CorpusCapabilityDocument]) -> Result<(), CorpusManifestError> {
    let document_ids: Vec<String> = documents
        .iter()
        .map(|document| document.document_id.clone())
        .collect();
    if let Some(index) = first_unsorted_unique(&document_ids) {
        return Err(invalid_field(
            "documents.document_id",
            "strictly sorted unique identifiers",
            &documents[index].document_id,
        ));
    }
    for document in documents {
        validate_document(document)?;
    }
    Ok(())
}

fn validate_document(document: &CorpusCapabilityDocument) -> Result<(), CorpusManifestError> {
    if !is_identifier(&document.document_id) {
        return Err(invalid_field(
            "documents.document_id",
            "nonempty identifier",
            &document.document_id,
        ));
    }
    if !is_uri(&document.document_uri) {
        return Err(invalid_field(
            "documents.document_uri",
            "capability document URI",
            &document.document_uri,
        ));
    }
    if !is_logical_path(&document.logical_path) {
        return Err(invalid_field(
            "documents.logical_path",
            "normalized relative logical path",
            &document.logical_path,
        ));
    }
    validate_hash("documents.sha256", &document.sha256, &document.logical_path)?;
    Ok(())
}

fn validate_path_uniqueness(manifest: &CorpusManifest) -> Result<(), CorpusManifestError> {
    let mut paths = BTreeSet::new();
    for path in manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.logical_path.as_str())
        .chain(
            manifest
                .documents
                .iter()
                .map(|document| document.logical_path.as_str()),
        )
    {
        if !paths.insert(path) {
            return Err(invalid_field("corpus", "logical path used once", path));
        }
    }
    Ok(())
}

fn validate_hash(field: &'static str, hash: &str, path: &str) -> Result<(), CorpusManifestError> {
    let expected = match sha256_validity(hash) {
        HashValidity::Valid => return Ok(()),
        HashValidity::Invalid => "64 hexadecimal SHA-256 digits",
        HashValidity::AllZero => "nonzero SHA-256",
    };
    Err(invalid_field(field, expected, path))
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> CorpusManifestError {
    CorpusManifestError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Corpus-manifest decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum CorpusManifestError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("corpus field {field} mismatch: expected {expected}, observed {observed}")]
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

    fn hash(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn corpus() -> CorpusManifest {
        CorpusManifest {
            schema: CORPUS_MANIFEST_SCHEMA.to_owned(),
            schema_version: CORPUS_MANIFEST_VERSION,
            manifest_id: "arbitrary-corpus-name".to_owned(),
            corpus_id: "arbitrary-corpus-family".to_owned(),
            artifacts: vec![CorpusArtifact {
                logical_path: "library/fixture-a.bin".to_owned(),
                sha256: hash('1'),
                bytes: 42,
            }],
            documents: vec![CorpusCapabilityDocument {
                document_id: "recipe-alpha".to_owned(),
                document_uri: "urn:real-disunity:capability:recipe-alpha".to_owned(),
                logical_path: "authoring/recipe-alpha.json".to_owned(),
                sha256: hash('2'),
                bytes: 7,
            }],
        }
    }

    #[test]
    fn corpus_round_trip_is_deterministic_with_arbitrary_names() {
        let manifest = corpus();
        let bytes = manifest.to_json_bytes().unwrap();
        assert_eq!(manifest.to_json_bytes().unwrap(), bytes);
        assert_eq!(CorpusManifest::from_json(&bytes).unwrap(), manifest);
        assert_eq!(manifest.document("recipe-alpha").map(|d| d.bytes), Some(7));
        assert!(manifest.artifact("library/fixture-a.bin").is_some());
    }

    #[test]
    fn zero_byte_entries_are_pinnable_with_the_exact_empty_digest() {
        // Zero-byte content is a valid pinned stream whose exact digest is the empty-input
        // SHA-256; whether emptiness is meaningful is judged by document codecs, not here.
        const EMPTY_SHA256: &str =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let mut empty_artifact = corpus();
        empty_artifact.artifacts[0].bytes = 0;
        empty_artifact.artifacts[0].sha256 = EMPTY_SHA256.to_owned();
        assert!(empty_artifact.validate().is_ok());

        let mut empty_document = corpus();
        empty_document.documents[0].bytes = 0;
        empty_document.documents[0].sha256 = EMPTY_SHA256.to_owned();
        assert!(empty_document.validate().is_ok());
    }

    #[test]
    fn corpus_identities_are_independent_of_any_save() {
        let mut sibling = corpus();
        sibling.manifest_id = "another-arbitrary-corpus".to_owned();
        sibling.artifacts[0].sha256 = hash('3');
        assert!(sibling.validate().is_ok());
        assert_ne!(sibling.manifest_id, corpus().manifest_id);

        let value: Value = serde_json::from_slice(&corpus().to_json_bytes().unwrap()).unwrap();
        let text = value.to_string();
        assert!(!text.contains("save"));
        assert!(!text.contains("lock"));
        assert!(!text.contains("receipt"));
    }

    #[test]
    fn duplicate_paths_across_lists_are_rejected() {
        let mut manifest = corpus();
        manifest.documents[0].logical_path = "library/fixture-a.bin".to_owned();
        assert!(matches!(
            manifest.validate(),
            Err(CorpusManifestError::FieldMismatch {
                field: "corpus",
                ..
            })
        ));
    }

    #[test]
    fn duplicate_ids_unsafe_paths_and_zero_hashes_are_rejected() {
        let mut duplicate_id = corpus();
        duplicate_id
            .documents
            .push(duplicate_id.documents[0].clone());
        assert!(matches!(
            duplicate_id.validate(),
            Err(CorpusManifestError::FieldMismatch {
                field: "documents.document_id",
                ..
            })
        ));

        let mut unsafe_path = corpus();
        unsafe_path.artifacts[0].logical_path = "../outside.bin".to_owned();
        assert!(matches!(
            unsafe_path.validate(),
            Err(CorpusManifestError::FieldMismatch {
                field: "artifacts.logical_path",
                ..
            })
        ));

        let mut zero_hash = corpus();
        zero_hash.artifacts[0].sha256 = hash('0');
        assert!(matches!(
            zero_hash.validate(),
            Err(CorpusManifestError::FieldMismatch {
                field: "artifacts.sha256",
                ..
            })
        ));
    }

    #[test]
    fn empty_corpus_is_rejected() {
        let mut manifest = corpus();
        manifest.artifacts.clear();
        manifest.documents.clear();
        assert!(matches!(
            manifest.validate(),
            Err(CorpusManifestError::FieldMismatch {
                field: "corpus",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let mut text = String::from_utf8(corpus().to_json_bytes().unwrap()).unwrap();
        text = text.replace("\"corpus_id\"", "\"unexpected_field\": 1, \"corpus_id\"");
        assert!(matches!(
            CorpusManifest::from_json(text.as_bytes()),
            Err(CorpusManifestError::Document(VersionedDocumentError::Json(
                _
            )))
        ));

        let text = String::from_utf8(corpus().to_json_bytes().unwrap())
            .unwrap()
            .replace("\"schema_version\":2", "\"schema_version\":3");
        assert!(matches!(
            CorpusManifest::from_json(text.as_bytes()),
            Err(CorpusManifestError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }
}
