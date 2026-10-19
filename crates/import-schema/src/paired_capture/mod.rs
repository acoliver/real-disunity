//! Source-neutral pairing of one observed capture with one replayed capture.
//!
//! This document pairs one capture observed at an original source build with one capture
//! replayed through this engine, so later differential measurement knows exactly which two
//! frames it may compare and on what terms. It is deliberately source-neutral: the manifest
//! names no product, path, save name, or personal metadata, records private inputs only by
//! content digest, and describes poses and state through typed records with explicit
//! known/unknown values.
//!
//! The contract fixes canonical units so numeric comparison is meaningful: positions are
//! metres, orientation is a normalized `x, y, z, w` quaternion, and vertical fields of view
//! are degrees. Observer adapters convert source-native units before writing a manifest; a
//! pose whose coordinate frame is not identified cannot be compared numerically and caps the
//! comparability grade rather than being silently compared.
//!
//! Every state variable the contract requires must be addressed, either as a known value or
//! as an explicit unknown with a reason; omission is never silent. Validation fails closed:
//! a claimed comparability grade stronger than the manifest's own evidence supports is
//! rejected, so mismatched frames cannot be quietly compared.

mod geometry;
mod grade;
mod observation;
mod side;

pub use geometry::{CameraPose, PoseRecord, ProjectionRecord, ViewportRecord};
pub use grade::{ComparabilityClaim, ComparabilityGrade, PoseTolerance};
pub use observation::{CORE_STATE_KEYS, CaptureState, Observed};
pub use side::{BuildIdentity, CaptureSide, ObservationRole, OutputArtifact};

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::validation::{HashValidity, is_identifier, sha256_validity};
use crate::version::{
    PAIRED_CAPTURE_MANIFEST_SCHEMA, PAIRED_CAPTURE_MANIFEST_VERSION, VersionedDocumentError,
    parse_versioned, to_canonical_vec, to_pretty_vec, verify_header,
};

/// A source-neutral pairing of one observed source capture with one replayed capture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedCaptureManifest {
    /// The exact schema of this manifest.
    pub schema: String,
    /// The schema version.
    pub schema_version: u32,
    /// The stable identity of this manifest.
    pub manifest_id: String,
    /// The source-side capture.
    pub source: CaptureSide,
    /// The replay-side capture.
    pub replay: CaptureSide,
    /// The comparability claim.
    pub comparability: ComparabilityClaim,
}

impl PairedCaptureManifest {
    /// Parses and validates a paired-capture manifest.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or paired-capture validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, PairedCaptureError> {
        let manifest: Self = parse_versioned(
            bytes,
            PAIRED_CAPTURE_MANIFEST_SCHEMA,
            PAIRED_CAPTURE_MANIFEST_VERSION,
        )?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serializes this manifest as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, PairedCaptureError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this manifest as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, PairedCaptureError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Returns the SHA-256 of the canonical bytes, content-addressing this manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn content_digest(&self) -> Result<String, PairedCaptureError> {
        Ok(sha256_hex(&self.to_json_bytes()?))
    }

    /// Derives the strongest comparability grade this manifest's own evidence supports.
    ///
    /// The derivation is conservative: any value that is absent, unknown, unregistered, or
    /// outside tolerance caps the grade at the boundary below it, and non-finite arithmetic
    /// caps rather than passes. The claimed grade may legitimately be weaker.
    #[must_use]
    pub fn derived_grade(&self) -> ComparabilityGrade {
        grade::derive_grade(
            &self.source,
            &self.replay,
            self.comparability.pose_tolerance,
        )
    }

    /// Validates the envelope, both sides, the claim, and the claimed-versus-derived grade.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed paired-capture violation.
    pub fn validate(&self) -> Result<(), PairedCaptureError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            PAIRED_CAPTURE_MANIFEST_SCHEMA,
            PAIRED_CAPTURE_MANIFEST_VERSION,
        )?;
        if !is_identifier(&self.manifest_id) {
            return Err(invalid_field(
                "manifest_id",
                "nonempty identifier",
                &self.manifest_id,
            ));
        }
        self.source.validate_for(ObservationRole::Source)?;
        self.replay.validate_for(ObservationRole::Replay)?;
        self.comparability.validate()?;
        let derived = self.derived_grade();
        if self.comparability.claimed_grade > derived {
            return Err(invalid_field(
                "comparability.claimed_grade",
                derived.as_str(),
                self.comparability.claimed_grade.as_str(),
            ));
        }
        Ok(())
    }
}

/// Validates one SHA-256 field.
fn validate_hash(field: &'static str, hash: &str) -> Result<(), PairedCaptureError> {
    let expected = match sha256_validity(hash) {
        HashValidity::Valid => return Ok(()),
        HashValidity::Invalid => "64 hexadecimal SHA-256 digits",
        HashValidity::AllZero => "nonzero SHA-256",
    };
    Err(invalid_field(field, expected, hash))
}

/// Formats a SHA-256 digest as lowercase hexadecimal.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // Writing into a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> PairedCaptureError {
    PairedCaptureError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Paired-capture manifest decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum PairedCaptureError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("paired-capture field {field} mismatch: expected {expected}, observed {observed}")]
    FieldMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
}

#[cfg(test)]
mod tests;
