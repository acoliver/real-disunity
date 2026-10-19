//! Standalone verification of normal-import authority for a verified publication.

use sha2::{Digest as _, Sha256};
use thiserror::Error;
use worldrunner_import_schema::{
    AttestationOutcome, CorpusManifest, CorpusManifestError, EnvironmentScopePolicy, ImportReceipt,
    ImportReceiptError, ReceiptPromotionError, SaveBundleError, SaveBundleManifest,
    ScopePolicyError, SourceLockError, SourceObservationLock, ValidationAttestation,
    ValidationAttestationError, require_promotion_equivalence,
};

use crate::VerifiedPublication;

/// Exact external authority documents required to activate one normal publication.
///
/// These documents remain independent authorities. The verifier hashes their
/// canonical authored bytes and checks the complete candidate-to-normal
/// governance chain; it never infers a document from a path or receipt string.
#[derive(Clone, Copy)]
pub struct NormalAuthorityDocuments<'a> {
    pub source_manifest: &'a SaveBundleManifest,
    pub source_lock: &'a SourceObservationLock,
    pub corpus_manifest: &'a CorpusManifest,
    pub scope_policy: &'a EnvironmentScopePolicy,
    pub candidate_receipt: &'a ImportReceipt,
    pub attestation: &'a ValidationAttestation,
}

/// Opaque evidence that a verified publication carries an exact normal authority chain.
#[derive(Debug)]
pub struct VerifiedNormalPublication {
    publication: VerifiedPublication,
}

impl VerifiedNormalPublication {
    /// Returns the exact verified publication closure.
    pub fn publication(&self) -> &VerifiedPublication {
        &self.publication
    }

    /// Consumes the authority evidence and returns the verified publication.
    pub fn into_publication(self) -> VerifiedPublication {
        self.publication
    }
}

/// Verifies external normal-import authority against an exact publication receipt.
///
/// # Errors
///
/// Returns a typed schema error for any invalid authority document, or an
/// authority-chain error for any stale digest, non-candidate receipt, or
/// non-approved/misbound attestation.
pub fn verify_normal_authority(
    publication: VerifiedPublication,
    documents: NormalAuthorityDocuments<'_>,
) -> Result<VerifiedNormalPublication, ActivationError> {
    let digests = canonical_authority_digests(&documents)?;
    verify_receipt_inputs(publication.receipt(), &digests)?;
    verify_candidate_receipt(documents.candidate_receipt, &digests)?;
    require_promotion_equivalence(documents.candidate_receipt, publication.receipt())
        .map_err(|source| ActivationError::CandidateResultDrift { source })?;
    verify_attestation(documents.attestation, publication.receipt(), &digests)?;
    Ok(VerifiedNormalPublication { publication })
}

struct AuthorityDigests {
    source_manifest: String,
    source_lock: String,
    corpus_manifest: String,
    scope_policy: String,
    candidate_receipt: String,
    attestation: String,
}

fn canonical_authority_digests(
    documents: &NormalAuthorityDocuments<'_>,
) -> Result<AuthorityDigests, ActivationError> {
    Ok(AuthorityDigests {
        source_manifest: digest(
            &documents
                .source_manifest
                .to_json_bytes()
                .map_err(|source| ActivationError::SourceManifest { source })?,
        ),
        source_lock: digest(
            &documents
                .source_lock
                .to_json_bytes()
                .map_err(|source| ActivationError::SourceLock { source })?,
        ),
        corpus_manifest: digest(
            &documents
                .corpus_manifest
                .to_json_bytes()
                .map_err(|source| ActivationError::CorpusManifest { source })?,
        ),
        scope_policy: digest(
            &documents
                .scope_policy
                .to_json_bytes()
                .map_err(|source| ActivationError::ScopePolicy { source })?,
        ),
        candidate_receipt: digest(
            &documents
                .candidate_receipt
                .to_json_bytes()
                .map_err(|source| ActivationError::CandidateReceipt { source })?,
        ),
        attestation: digest(
            &documents
                .attestation
                .to_json_bytes()
                .map_err(|source| ActivationError::Attestation { source })?,
        ),
    })
}

fn verify_receipt_inputs(
    receipt: &ImportReceipt,
    digests: &AuthorityDigests,
) -> Result<(), ActivationError> {
    require_equal(
        "source_lock_digest",
        &digests.source_lock,
        &receipt.source_lock_digest,
    )?;
    require_equal(
        "source_manifest_digest",
        &digests.source_manifest,
        &receipt.source_manifest_digest,
    )?;
    require_equal(
        "corpus_manifest_digest",
        &digests.corpus_manifest,
        &receipt.corpus_manifest_digest,
    )?;
    require_equal(
        "scope_policy_digest",
        &digests.scope_policy,
        &receipt.scope_policy_digest,
    )?;
    let observed = receipt
        .authorization_receipt_digest
        .as_deref()
        .unwrap_or("absent");
    require_equal(
        "authorization_receipt_digest",
        &digests.candidate_receipt,
        observed,
    )
}

fn verify_candidate_receipt(
    receipt: &ImportReceipt,
    digests: &AuthorityDigests,
) -> Result<(), ActivationError> {
    require_equal(
        "candidate.source_lock_digest",
        &digests.source_lock,
        &receipt.source_lock_digest,
    )?;
    require_equal(
        "candidate.source_manifest_digest",
        &digests.source_manifest,
        &receipt.source_manifest_digest,
    )?;
    require_equal(
        "candidate.corpus_manifest_digest",
        &digests.corpus_manifest,
        &receipt.corpus_manifest_digest,
    )?;
    require_equal(
        "candidate.scope_policy_digest",
        &digests.scope_policy,
        &receipt.scope_policy_digest,
    )?;
    if !receipt.validation_passed {
        return Err(ActivationError::InvalidCandidateReceipt {
            field: "validation_passed",
            observed: "false".to_owned(),
        });
    }
    if receipt.authorization_receipt_digest.is_some() {
        return Err(ActivationError::InvalidCandidateReceipt {
            field: "authorization_receipt_digest",
            observed: "present".to_owned(),
        });
    }
    if !receipt.attestation_digests.is_empty() {
        return Err(ActivationError::InvalidCandidateReceipt {
            field: "attestation_digests",
            observed: "present".to_owned(),
        });
    }
    Ok(())
}

fn verify_attestation(
    attestation: &ValidationAttestation,
    normal_receipt: &ImportReceipt,
    digests: &AuthorityDigests,
) -> Result<(), ActivationError> {
    if attestation.outcome != AttestationOutcome::Approved {
        return Err(ActivationError::AttestationNotApproved);
    }
    require_equal(
        "attestation.subject_lock_digest",
        &digests.source_lock,
        &attestation.subject_lock_digest,
    )?;
    require_equal(
        "attestation.subject_receipt_digest",
        &digests.candidate_receipt,
        &attestation.subject_receipt_digest,
    )?;
    require_attestation_closure(&normal_receipt.attestation_digests, &digests.attestation)
}

fn require_attestation_closure(observed: &[String], expected: &str) -> Result<(), ActivationError> {
    if observed == [expected] {
        Ok(())
    } else {
        Err(ActivationError::AttestationClosureMismatch {
            expected: expected.to_owned(),
            observed: observed.to_vec(),
        })
    }
}

fn require_equal(
    field: &'static str,
    expected: &str,
    observed: &str,
) -> Result<(), ActivationError> {
    if expected == observed {
        Ok(())
    } else {
        Err(ActivationError::AuthorityDrift {
            field,
            expected: expected.to_owned(),
            observed: observed.to_owned(),
        })
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Standalone normal-publication authority verification failures.
#[derive(Debug, Error)]
pub enum ActivationError {
    #[error("source manifest is invalid: {source}")]
    SourceManifest { source: SaveBundleError },
    #[error("source lock is invalid: {source}")]
    SourceLock { source: SourceLockError },
    #[error("corpus manifest is invalid: {source}")]
    CorpusManifest { source: CorpusManifestError },
    #[error("scope policy is invalid: {source}")]
    ScopePolicy { source: ScopePolicyError },
    #[error("candidate receipt is invalid: {source}")]
    CandidateReceipt { source: ImportReceiptError },
    #[error("validation attestation is invalid: {source}")]
    Attestation { source: ValidationAttestationError },
    #[error("normal publication differs from its approved candidate result: {source}")]
    CandidateResultDrift { source: ReceiptPromotionError },
    #[error("normal authority field {field} drift: expected {expected}, observed {observed}")]
    AuthorityDrift {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error("candidate receipt field {field} is invalid: observed {observed}")]
    InvalidCandidateReceipt {
        field: &'static str,
        observed: String,
    },
    #[error(
        "normal receipt must cite exactly the independently verified attestation {expected}, observed {observed:?}"
    )]
    AttestationClosureMismatch {
        expected: String,
        observed: Vec<String>,
    },
    #[error("validation attestation is not approved")]
    AttestationNotApproved,
}

#[cfg(test)]
mod tests {
    use super::{ActivationError, require_attestation_closure};

    #[test]
    fn attestation_closure_requires_one_exact_digest() {
        let expected = "a".repeat(64);
        assert!(require_attestation_closure(std::slice::from_ref(&expected), &expected).is_ok());
        for observed in [
            Vec::new(),
            vec!["b".repeat(64)],
            vec![expected.clone(), "b".repeat(64)],
            vec!["b".repeat(64), expected.clone()],
        ] {
            assert!(matches!(
                require_attestation_closure(&observed, &expected),
                Err(ActivationError::AttestationClosureMismatch { .. })
            ));
        }
    }
}
