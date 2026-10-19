use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SAVE_BUNDLE_MANIFEST_SCHEMA: &str = "real-disunity.save-bundle-manifest.v1";
pub const SOURCE_OBSERVATION_LOCK_SCHEMA: &str = "real-disunity.source-observation-lock.v1";
pub const CORPUS_MANIFEST_SCHEMA: &str = "real-disunity.corpus-manifest.v2";
pub const CORPUS_BINDING_SCHEMA: &str = "real-disunity.corpus-binding.v1";
pub const CORPUS_BINDING_DOCUMENT_URI: &str = "real-disunity:corpus-binding-document-v1";
pub const CAPABILITY_GRAPH_SCHEMA: &str = "real-disunity.capability-graph.v1";
pub const CAPABILITY_GRAPH_DOCUMENT_URI: &str = "real-disunity:capability-graph-document-v1";
pub const ENVIRONMENT_SCOPE_POLICY_SCHEMA: &str = "real-disunity.environment-scope-policy.v1";
pub const VALIDATION_ATTESTATION_SCHEMA: &str = "real-disunity.validation-attestation.v2";
pub const IMPORT_RECEIPT_SCHEMA: &str = "real-disunity.import-receipt.v4";
pub const PUBLICATION_SET_SCHEMA: &str = "real-disunity.publication-set.v1";
pub const PAIRED_CAPTURE_MANIFEST_SCHEMA: &str = "real-disunity.paired-capture-manifest.v1";

pub const SAVE_BUNDLE_MANIFEST_VERSION: u32 = 1;
pub const SOURCE_OBSERVATION_LOCK_VERSION: u32 = 1;
pub const CORPUS_MANIFEST_VERSION: u32 = 2;
pub const CORPUS_BINDING_VERSION: u32 = 1;
pub(crate) const ENVIRONMENT_SCOPE_POLICY_VERSION: u32 = 1;
pub const CAPABILITY_GRAPH_VERSION: u32 = 1;
pub const VALIDATION_ATTESTATION_VERSION: u32 = 2;
pub const IMPORT_RECEIPT_VERSION: u32 = 4;
pub const PUBLICATION_SET_VERSION: u32 = 1;
pub const PAIRED_CAPTURE_MANIFEST_VERSION: u32 = 1;

/// Version envelope shared by every import document.
///
/// This view intentionally tolerates unknown fields so document schemas can be inspected before
/// the full strict payload is decoded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentHeader {
    pub schema: String,
    pub schema_version: u32,
}

/// Fail-closed decoding and schema-version errors.
#[derive(Debug, Error)]
pub enum VersionedDocumentError {
    #[error("document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schema tag mismatch: expected {expected}, observed {observed}")]
    SchemaTagMismatch {
        expected: &'static str,
        observed: String,
    },
    #[error("unsupported schema version: expected {expected}, observed {observed}")]
    UnsupportedSchemaVersion { expected: u32, observed: u32 },
}

pub(crate) fn parse_versioned<T>(
    bytes: &[u8],
    expected_schema: &'static str,
    expected_version: u32,
) -> Result<T, VersionedDocumentError>
where
    T: DeserializeOwned,
{
    let header: DocumentHeader = serde_json::from_slice(bytes)?;
    verify_header(&header, expected_schema, expected_version)?;
    Ok(serde_json::from_slice(bytes)?)
}

pub(crate) fn verify_header(
    header: &DocumentHeader,
    expected_schema: &'static str,
    expected_version: u32,
) -> Result<(), VersionedDocumentError> {
    if header.schema != expected_schema {
        return Err(VersionedDocumentError::SchemaTagMismatch {
            expected: expected_schema,
            observed: header.schema.clone(),
        });
    }
    if header.schema_version != expected_version {
        return Err(VersionedDocumentError::UnsupportedSchemaVersion {
            expected: expected_version,
            observed: header.schema_version,
        });
    }
    Ok(())
}

pub(crate) fn to_pretty_vec<T>(document: &T) -> Result<Vec<u8>, VersionedDocumentError>
where
    T: Serialize,
{
    Ok(serde_json::to_vec_pretty(document)?)
}

/// Serializes a document as compact deterministic JSON.
///
/// Determinism relies on every document using ordered maps, fixed struct field order, and no
/// unstructured collections, so identical documents always produce identical bytes.
pub(crate) fn to_canonical_vec<T>(document: &T) -> Result<Vec<u8>, VersionedDocumentError>
where
    T: Serialize,
{
    Ok(serde_json::to_vec(document)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_document_is_rejected() {
        let error = parse_versioned::<DocumentHeader>(b"{", CORPUS_MANIFEST_SCHEMA, 2)
            .expect_err("truncated JSON must fail");
        assert!(matches!(error, VersionedDocumentError::Json(_)));
    }

    #[test]
    fn wrong_schema_is_rejected_before_payload_decoding() {
        let bytes = br#"{"schema":"real-disunity.other.v1","schema_version":1}"#;
        let error = parse_versioned::<DocumentHeader>(bytes, CORPUS_MANIFEST_SCHEMA, 2)
            .expect_err("wrong schema must fail");
        assert!(matches!(
            error,
            VersionedDocumentError::SchemaTagMismatch { .. }
        ));
    }

    #[test]
    fn unknown_version_is_rejected() {
        let bytes = br#"{"schema":"real-disunity.corpus-manifest.v2","schema_version":3}"#;
        let error = parse_versioned::<DocumentHeader>(bytes, CORPUS_MANIFEST_SCHEMA, 2)
            .expect_err("future schema version must fail");
        assert!(matches!(
            error,
            VersionedDocumentError::UnsupportedSchemaVersion {
                expected: 2,
                observed: 3
            }
        ));
    }

    #[test]
    fn header_round_trip_is_stable() {
        let header = DocumentHeader {
            schema: CORPUS_MANIFEST_SCHEMA.to_owned(),
            schema_version: CORPUS_MANIFEST_VERSION,
        };
        let bytes = to_pretty_vec(&header).unwrap();
        let parsed = parse_versioned::<DocumentHeader>(&bytes, CORPUS_MANIFEST_SCHEMA, 2).unwrap();
        assert_eq!(parsed, header);
        assert_eq!(
            to_canonical_vec(&header).unwrap(),
            to_canonical_vec(&header).unwrap()
        );
    }
}
