use crate::validation::{
    HashValidity, is_identifier, is_nonempty_text, sha256_matches_empty_length, sha256_validity,
};
use crate::version::{
    VALIDATION_ATTESTATION_SCHEMA, VALIDATION_ATTESTATION_VERSION, VersionedDocumentError,
    parse_versioned, to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Governance decision recorded by an attestation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationOutcome {
    Approved,
    Rejected,
    Conditional,
}

/// Content address of one independently retained validation-evidence artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestationEvidenceArtifact {
    pub evidence_id: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Independent governance record over one source lock and candidate import receipt.
///
/// The attestation cites the digests it governs and content-addresses the exact evidence bytes
/// supporting its reviewer decision. Governance remains outside source identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationAttestation {
    pub schema: String,
    pub schema_version: u32,
    pub attestation_id: String,
    pub subject_lock_digest: String,
    pub subject_receipt_digest: String,
    pub outcome: AttestationOutcome,
    pub reviewer: String,
    pub reason: String,
    pub evidence: Vec<AttestationEvidenceArtifact>,
}

impl ValidationAttestation {
    /// Parses and validates a validation attestation.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or attestation validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ValidationAttestationError> {
        let attestation: Self = parse_versioned(
            bytes,
            VALIDATION_ATTESTATION_SCHEMA,
            VALIDATION_ATTESTATION_VERSION,
        )?;
        attestation.validate()?;
        Ok(attestation)
    }

    /// Serializes this attestation as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the attestation is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, ValidationAttestationError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this attestation as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the attestation is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, ValidationAttestationError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Returns whether this attestation governs the given lock and receipt digests.
    #[must_use]
    pub fn binds(&self, subject_lock_digest: &str, subject_receipt_digest: &str) -> bool {
        self.subject_lock_digest == subject_lock_digest
            && self.subject_receipt_digest == subject_receipt_digest
    }

    /// Validates identity, subject digests, citations, and evidence ordering.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed attestation violation.
    pub fn validate(&self) -> Result<(), ValidationAttestationError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            VALIDATION_ATTESTATION_SCHEMA,
            VALIDATION_ATTESTATION_VERSION,
        )?;
        if !is_identifier(&self.attestation_id) {
            return Err(invalid_field(
                "attestation_id",
                "nonempty identifier",
                &self.attestation_id,
            ));
        }
        validate_hash("subject_lock_digest", &self.subject_lock_digest)?;
        validate_hash("subject_receipt_digest", &self.subject_receipt_digest)?;
        if !is_nonempty_text(&self.reviewer) {
            return Err(invalid_field(
                "reviewer",
                "nonempty reviewer",
                &self.reviewer,
            ));
        }
        if !is_nonempty_text(&self.reason) {
            return Err(invalid_field("reason", "nonempty reason", &self.reason));
        }
        validate_evidence(&self.evidence)
    }
}

fn validate_evidence(
    evidence: &[AttestationEvidenceArtifact],
) -> Result<(), ValidationAttestationError> {
    if evidence.is_empty() {
        return Err(invalid_field(
            "evidence",
            "at least one content-addressed artifact",
            "empty",
        ));
    }
    let mut previous = None;
    for artifact in evidence {
        if !is_identifier(&artifact.evidence_id) {
            return Err(invalid_field(
                "evidence.evidence_id",
                "nonempty identifier",
                &artifact.evidence_id,
            ));
        }
        if previous.is_some_and(|id| id >= artifact.evidence_id.as_str()) {
            return Err(invalid_field(
                "evidence",
                "strictly sorted unique evidence identifiers",
                &artifact.evidence_id,
            ));
        }
        validate_hash("evidence.sha256", &artifact.sha256)?;
        if !sha256_matches_empty_length(&artifact.sha256, artifact.bytes) {
            return Err(invalid_field(
                "evidence.sha256",
                "SHA-256 of empty content when bytes is zero",
                &artifact.sha256,
            ));
        }
        previous = Some(artifact.evidence_id.as_str());
    }
    Ok(())
}

fn validate_hash(field: &'static str, hash: &str) -> Result<(), ValidationAttestationError> {
    let expected = match sha256_validity(hash) {
        HashValidity::Valid => return Ok(()),
        HashValidity::Invalid => "64 hexadecimal SHA-256 digits",
        HashValidity::AllZero => "nonzero SHA-256",
    };
    Err(invalid_field(field, expected, hash))
}

fn invalid_field(
    field: &'static str,
    expected: &str,
    observed: &str,
) -> ValidationAttestationError {
    ValidationAttestationError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Attestation decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum ValidationAttestationError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("attestation field {field} mismatch: expected {expected}, observed {observed}")]
    FieldMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn attestation() -> ValidationAttestation {
        ValidationAttestation {
            schema: VALIDATION_ATTESTATION_SCHEMA.to_owned(),
            schema_version: VALIDATION_ATTESTATION_VERSION,
            attestation_id: "arbitrary-review-record".to_owned(),
            subject_lock_digest: hash('1'),
            subject_receipt_digest: hash('2'),
            outcome: AttestationOutcome::Approved,
            reviewer: "arbitrary-reviewer".to_owned(),
            reason: "arbitrary governance reason".to_owned(),
            evidence: vec![AttestationEvidenceArtifact {
                evidence_id: "capture-a".to_owned(),
                sha256: hash('3'),
                bytes: 17,
            }],
        }
    }

    #[test]
    fn attestation_round_trip_is_deterministic_and_binds_subjects() {
        let record = attestation();
        let bytes = record.to_json_bytes().unwrap();
        assert_eq!(record.to_json_bytes().unwrap(), bytes);
        assert_eq!(ValidationAttestation::from_json(&bytes).unwrap(), record);
        assert!(record.binds(&hash('1'), &hash('2')));
        assert!(!record.binds(&hash('1'), &hash('3')));
    }

    #[test]
    fn empty_reason_reviewer_and_evidence_are_rejected() {
        let mut empty_reason = attestation();
        empty_reason.reason = "   ".to_owned();
        assert!(matches!(
            empty_reason.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "reason",
                ..
            })
        ));

        let mut empty_reviewer = attestation();
        empty_reviewer.reviewer.clear();
        assert!(matches!(
            empty_reviewer.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "reviewer",
                ..
            })
        ));

        let mut empty_evidence = attestation();
        empty_evidence.evidence.clear();
        assert!(matches!(
            empty_evidence.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "evidence",
                ..
            })
        ));
    }

    #[test]
    fn malformed_subject_digests_and_unsorted_evidence_are_rejected() {
        let mut zero_digest = attestation();
        zero_digest.subject_lock_digest = hash('0');
        assert!(matches!(
            zero_digest.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "subject_lock_digest",
                ..
            })
        ));

        let mut unsorted = attestation();
        unsorted.evidence = vec![
            AttestationEvidenceArtifact {
                evidence_id: "zeta".to_owned(),
                sha256: hash('3'),
                bytes: 1,
            },
            AttestationEvidenceArtifact {
                evidence_id: "alpha".to_owned(),
                sha256: hash('4'),
                bytes: 1,
            },
        ];
        assert!(matches!(
            unsorted.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "evidence",
                ..
            })
        ));
    }

    #[test]
    fn evidence_addresses_reject_invalid_identity_and_digest_but_accept_empty_content() {
        let mut invalid_id = attestation();
        invalid_id.evidence[0].evidence_id = "not an identifier".to_owned();
        assert!(matches!(
            invalid_id.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "evidence.evidence_id",
                ..
            })
        ));

        let mut zero_digest = attestation();
        zero_digest.evidence[0].sha256 = hash('0');
        assert!(matches!(
            zero_digest.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "evidence.sha256",
                ..
            })
        ));

        let mut malformed_digest = attestation();
        malformed_digest.evidence[0].sha256 = "not-a-digest".to_owned();
        assert!(matches!(
            malformed_digest.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "evidence.sha256",
                ..
            })
        ));

        let mut impossible_empty_content = attestation();
        impossible_empty_content.evidence[0].bytes = 0;
        assert!(matches!(
            impossible_empty_content.validate(),
            Err(ValidationAttestationError::FieldMismatch {
                field: "evidence.sha256",
                ..
            })
        ));

        let mut empty_content = attestation();
        empty_content.evidence[0].bytes = 0;
        empty_content.evidence[0].sha256 =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned();
        assert!(empty_content.validate().is_ok());
    }

    #[test]
    fn citation_only_v1_attestations_fail_closed() {
        let legacy = br#"{
            "schema":"real-disunity.validation-attestation.v1",
            "schema_version":1,
            "attestation_id":"legacy-review",
            "subject_lock_digest":"1111111111111111111111111111111111111111111111111111111111111111",
            "subject_receipt_digest":"2222222222222222222222222222222222222222222222222222222222222222",
            "outcome":"approved",
            "reviewer":"legacy reviewer",
            "reason":"citation only",
            "evidence":["urn:legacy:citation"]
        }"#;
        assert!(ValidationAttestation::from_json(legacy).is_err());
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let mut text = String::from_utf8(attestation().to_json_bytes().unwrap()).unwrap();
        text = text.replace(
            "\"attestation_id\"",
            "\"unexpected_field\": 1, \"attestation_id\"",
        );
        assert!(matches!(
            ValidationAttestation::from_json(text.as_bytes()),
            Err(ValidationAttestationError::Document(
                VersionedDocumentError::Json(_)
            ))
        ));

        let text = String::from_utf8(attestation().to_json_bytes().unwrap())
            .unwrap()
            .replace("\"schema_version\":2", "\"schema_version\":4");
        assert!(matches!(
            ValidationAttestation::from_json(text.as_bytes()),
            Err(ValidationAttestationError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }
}
