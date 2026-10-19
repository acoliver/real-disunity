//! Universal semantic world model.
//!
//! This crate defines a runtime contract between decoding, authoring, and
//! generation layers: a deterministic [`SemanticValue`] algebra for payloads,
//! a corpus-free [`DecodedSave`] whose [`SourceIdentity`] is derived solely
//! from its canonical components, and a validated [`SemanticWorldDocument`]
//! whose content-addressed [`ResourceBinding`]s resolve every [`SemanticValue::BindingRef`].
//!
//! All digests are explicit canonical `SHA-256` computations over
//! length-prefixed domain-separated byte streams; serialized JSON bytes are
//! never hashed. Operational validation bounds are supplied explicitly by
//! callers via [`ValidationLimits`] because this crate embeds no
//! dataset-specific population assumptions.

mod canonical;
mod digest;
mod document;
mod identity;
mod validation;
mod value;

pub use canonical::{DOMAIN_GENERATED_STATE, DOMAIN_SOURCE_IDENTITY};
pub use digest::{ContentDigest, DigestError, GeneratedStateDigest, SHA256_LEN};
pub use document::{ResourceBinding, SchemaDescriptor, SemanticWorldDocument};
pub use identity::{DecodedSave, SemanticComponent, SourceIdentity};
pub use validation::{
    ValidationLimits, WorldModelError, validate_semantic_identifier, validate_structural_uri,
};
pub use value::{SemanticMap, SemanticValue};
