//! Typed fail-closed failures for authored render-contract documents.

use thiserror::Error;
use worldrunner_world_model::WorldModelError;

/// Failures raised while decoding or validating one authored render contract.
///
/// Every variant fails closed: there is no fallback, compatibility, or partial
/// acceptance path for authored documents.
#[derive(Debug, Error)]
pub enum ContractDocumentError {
    /// The document bytes are not valid strict JSON.
    #[error("document is not valid authored render-contract JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The schema tag names a different document family.
    #[error("schema tag mismatch: expected {expected:?}, observed {observed:?}")]
    SchemaTagMismatch {
        /// The exact schema tag this decoder supports.
        expected: &'static str,
        /// The schema tag the document carried.
        observed: String,
    },
    /// The schema version is unknown to this decoder.
    #[error("unsupported schema version: expected {expected}, observed {observed}")]
    UnsupportedSchemaVersion {
        /// The exact schema version this decoder supports.
        expected: u32,
        /// The schema version the document carried.
        observed: u32,
    },
    /// An identifier field violated the universal world-model identifier rules.
    #[error("invalid identifier in {field}: {source}")]
    InvalidIdentifier {
        /// The field carrying the malformed identifier.
        field: &'static str,
        /// The universal identifier validation failure.
        source: WorldModelError,
    },
    /// A URI field violated the universal world-model structural URI rules.
    #[error("invalid URI in {field}: {source}")]
    InvalidUri {
        /// The field carrying the malformed URI.
        field: &'static str,
        /// The universal URI validation failure.
        source: WorldModelError,
    },
    /// A keyed collection was not provided in strictly ascending key order.
    #[error("entries in {field} must be provided in strictly ascending key order")]
    UnsortedEntries {
        /// The collection whose keys arrived out of order.
        field: &'static str,
    },
    /// Two entries of one keyed collection share a key.
    #[error("duplicate key {key:?} in {field}")]
    DuplicateKey {
        /// The collection carrying the duplicated key.
        field: &'static str,
        /// The key value that appeared twice.
        key: String,
    },
    /// Checked arithmetic overflowed while validating one field.
    #[error("checked arithmetic overflowed while validating {field}")]
    ArithmeticOverflow {
        /// The field whose derived computation overflowed.
        field: &'static str,
    },
    /// A value fell outside its explicit universal bounds.
    #[error("value {observed} in {field} must lie within [{min}, {max}]")]
    OutOfRange {
        /// The field carrying the out-of-bounds value.
        field: &'static str,
        /// The inclusive minimum bound.
        min: i128,
        /// The inclusive maximum bound.
        max: i128,
        /// The observed value.
        observed: i128,
    },
    /// A mandatory collection or string was empty.
    #[error("collection {field} must not be empty")]
    EmptyCollection {
        /// The mandatory collection that arrived empty.
        field: &'static str,
    },
    /// A field-specific structural invariant was violated.
    #[error("invalid {field}: {reason}")]
    ViolatedInvariant {
        /// The field whose structural invariant failed.
        field: &'static str,
        /// The invariant the field violated.
        reason: &'static str,
    },
    /// An authored limit bound was exceeded by the authored content.
    #[error("{field} carries {observed} entries but the contract permits at most {max}")]
    LimitExceeded {
        /// The bounded collection that exceeded its authored limit.
        field: &'static str,
        /// The authored maximum.
        max: u64,
        /// The observed entry count.
        observed: u64,
    },
    /// An asset URI is declared by no asset resolution binding.
    #[error("asset URI {uri:?} in {collection} is declared by no asset binding")]
    UnresolvedAssetUri {
        /// The collection carrying the unresolved asset URI.
        collection: &'static str,
        /// The asset URI no binding declares.
        uri: String,
    },
}
