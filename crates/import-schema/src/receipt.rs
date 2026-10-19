use crate::census::{CensusSummary, CensusSummaryError};
use crate::validation::{
    HashValidity, first_unsorted_unique, is_identifier, is_logical_path,
    sha256_matches_empty_length, sha256_validity,
};
use crate::version::{
    IMPORT_RECEIPT_SCHEMA, IMPORT_RECEIPT_VERSION, VersionedDocumentError, parse_versioned,
    to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// One generated output artifact recorded in a receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptOutputArtifact {
    pub logical_path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Terminal record binding one import run to its inputs, outputs, and governance.
///
/// The receipt is produced by the importer and is never part of source identity: it cites the
/// source lock, corpus, and scope-policy digests, pins importer and codec revisions, records
/// generated world/state digests, output artifacts, the census summary, and validation outcomes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportReceipt {
    pub schema: String,
    pub schema_version: u32,
    pub receipt_id: String,
    pub source_lock_digest: String,
    pub source_manifest_digest: String,
    pub corpus_manifest_digest: String,
    pub scope_policy_digest: String,
    pub importer_revision: String,
    pub codec_revisions: BTreeMap<String, String>,
    pub source_identity_digest: String,
    pub state_digest: Option<String>,
    pub outputs: Vec<ReceiptOutputArtifact>,
    pub census: CensusSummary,
    pub validation_passed: bool,
    pub authorization_receipt_digest: Option<String>,
    pub attestation_digests: Vec<String>,
}

impl ImportReceipt {
    /// Parses and validates an import receipt.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, receipt, or census-summary validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ImportReceiptError> {
        let receipt: Self = parse_versioned(bytes, IMPORT_RECEIPT_SCHEMA, IMPORT_RECEIPT_VERSION)?;
        receipt.validate()?;
        Ok(receipt)
    }

    /// Serializes this receipt as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the receipt is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, ImportReceiptError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this receipt as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the receipt is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, ImportReceiptError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Returns whether this receipt binds the given input digests.
    #[must_use]
    pub fn binds(
        &self,
        source_lock_digest: &str,
        source_manifest_digest: &str,
        corpus_manifest_digest: &str,
        scope_policy_digest: &str,
    ) -> bool {
        self.source_lock_digest == source_lock_digest
            && self.source_manifest_digest == source_manifest_digest
            && self.corpus_manifest_digest == corpus_manifest_digest
            && self.scope_policy_digest == scope_policy_digest
    }

    /// Finds a recorded output by its normalized logical path.
    #[must_use]
    pub fn output(&self, logical_path: &str) -> Option<&ReceiptOutputArtifact> {
        self.outputs
            .binary_search_by_key(&logical_path, |output| output.logical_path.as_str())
            .ok()
            .map(|index| &self.outputs[index])
    }

    /// Validates identity, binding digests, revisions, outputs, census, and attestations.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed receipt violation.
    pub fn validate(&self) -> Result<(), ImportReceiptError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            IMPORT_RECEIPT_SCHEMA,
            IMPORT_RECEIPT_VERSION,
        )?;
        if !is_identifier(&self.receipt_id) {
            return Err(invalid_field(
                "receipt_id",
                "nonempty identifier",
                &self.receipt_id,
            ));
        }
        if !is_identifier(&self.importer_revision) {
            return Err(invalid_field(
                "importer_revision",
                "nonempty identifier",
                &self.importer_revision,
            ));
        }
        validate_binding_digests(self)?;
        validate_codec_revisions(&self.codec_revisions)?;
        validate_outputs(&self.outputs)?;
        self.census.validate()?;
        validate_attestation_digests(&self.attestation_digests)
    }
}

/// Requires a promoted normal receipt to preserve every approved candidate result fact.
///
/// Receipt identity and promotion-governance fields are deliberately excluded: the promoted
/// receipt has its own identifier and cites the candidate receipt and approving attestation.
/// Input authority, importer/codec revisions, selected semantic state, output closure, census,
/// and validation outcome must remain exact.
///
/// # Errors
///
/// Returns the first substantive field that differs between candidate and promoted receipts.
pub fn require_promotion_equivalence(
    candidate: &ImportReceipt,
    promoted: &ImportReceipt,
) -> Result<(), ReceiptPromotionError> {
    for (field, expected, observed) in [
        (
            "source_lock_digest",
            &candidate.source_lock_digest,
            &promoted.source_lock_digest,
        ),
        (
            "source_manifest_digest",
            &candidate.source_manifest_digest,
            &promoted.source_manifest_digest,
        ),
        (
            "corpus_manifest_digest",
            &candidate.corpus_manifest_digest,
            &promoted.corpus_manifest_digest,
        ),
        (
            "scope_policy_digest",
            &candidate.scope_policy_digest,
            &promoted.scope_policy_digest,
        ),
        (
            "importer_revision",
            &candidate.importer_revision,
            &promoted.importer_revision,
        ),
        (
            "source_identity_digest",
            &candidate.source_identity_digest,
            &promoted.source_identity_digest,
        ),
    ] {
        require_equal(field, expected, observed)?;
    }
    require_equal(
        "codec_revisions",
        &candidate.codec_revisions,
        &promoted.codec_revisions,
    )?;
    require_equal(
        "state_digest",
        &candidate.state_digest,
        &promoted.state_digest,
    )?;
    require_equal("outputs", &candidate.outputs, &promoted.outputs)?;
    require_equal("census", &candidate.census, &promoted.census)?;
    require_equal(
        "validation_passed",
        &candidate.validation_passed,
        &promoted.validation_passed,
    )
}

fn require_equal<T: PartialEq>(
    field: &'static str,
    expected: &T,
    observed: &T,
) -> Result<(), ReceiptPromotionError> {
    if expected == observed {
        Ok(())
    } else {
        Err(ReceiptPromotionError::ResultMismatch { field })
    }
}

/// Candidate-to-normal promotion equivalence errors.
#[derive(Debug, Error)]
pub enum ReceiptPromotionError {
    #[error("promoted receipt differs from the approved candidate result at {field}")]
    ResultMismatch { field: &'static str },
}

fn validate_binding_digests(receipt: &ImportReceipt) -> Result<(), ImportReceiptError> {
    validate_hash("source_lock_digest", &receipt.source_lock_digest)?;
    validate_hash("source_manifest_digest", &receipt.source_manifest_digest)?;
    validate_hash("corpus_manifest_digest", &receipt.corpus_manifest_digest)?;
    validate_hash("scope_policy_digest", &receipt.scope_policy_digest)?;
    validate_hash("source_identity_digest", &receipt.source_identity_digest)?;
    if let Some(state_digest) = &receipt.state_digest {
        validate_hash("state_digest", state_digest)?;
    }
    if let Some(authorization_receipt_digest) = &receipt.authorization_receipt_digest {
        validate_hash("authorization_receipt_digest", authorization_receipt_digest)?;
    }
    Ok(())
}

fn validate_codec_revisions(
    revisions: &BTreeMap<String, String>,
) -> Result<(), ImportReceiptError> {
    for (codec_id, revision) in revisions {
        if !is_identifier(codec_id) || !is_identifier(revision) {
            return Err(invalid_field(
                "codec_revisions",
                "identifier codec and revision",
                codec_id,
            ));
        }
    }
    Ok(())
}

fn validate_outputs(outputs: &[ReceiptOutputArtifact]) -> Result<(), ImportReceiptError> {
    let paths: Vec<String> = outputs
        .iter()
        .map(|output| output.logical_path.clone())
        .collect();
    if let Some(index) = first_unsorted_unique(&paths) {
        return Err(invalid_field(
            "outputs",
            "strictly sorted unique logical paths",
            &outputs[index].logical_path,
        ));
    }
    for output in outputs {
        if !is_logical_path(&output.logical_path) {
            return Err(invalid_field(
                "outputs.logical_path",
                "normalized relative logical path",
                &output.logical_path,
            ));
        }
        validate_hash("outputs.sha256", &output.sha256)?;
        if !sha256_matches_empty_length(&output.sha256, output.bytes) {
            return Err(invalid_field(
                "outputs.sha256",
                "SHA-256 of empty content when bytes is zero",
                &output.sha256,
            ));
        }
    }
    Ok(())
}

fn validate_attestation_digests(digests: &[String]) -> Result<(), ImportReceiptError> {
    if let Some(index) = first_unsorted_unique(digests) {
        return Err(invalid_field(
            "attestation_digests",
            "strictly sorted unique digests",
            &digests[index],
        ));
    }
    for digest in digests {
        validate_hash("attestation_digests", digest)?;
    }
    Ok(())
}

fn validate_hash(field: &'static str, hash: &str) -> Result<(), ImportReceiptError> {
    let expected = match sha256_validity(hash) {
        HashValidity::Valid => return Ok(()),
        HashValidity::Invalid => "64 hexadecimal SHA-256 digits",
        HashValidity::AllZero => "nonzero SHA-256",
    };
    Err(invalid_field(field, expected, hash))
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> ImportReceiptError {
    ImportReceiptError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Import-receipt decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum ImportReceiptError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error(transparent)]
    Census(#[from] CensusSummaryError),
    #[error("receipt field {field} mismatch: expected {expected}, observed {observed}")]
    FieldMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::census::{
        BindingClosureSummary, CensusBucket, CensusFunnel, CensusResolution, OptionalTextPartition,
    };

    fn hash(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn census() -> CensusSummary {
        CensusSummary {
            inventory_total: 2,
            bucket_totals: BTreeMap::from([
                (CensusBucket::Selected, 2),
                (CensusBucket::ExcludedByDesign, 0),
                (CensusBucket::Withheld, 0),
            ]),
            source_entry_totals: BTreeMap::from([("arbitrary-entry".to_owned(), 2)]),
            source_schema_totals: BTreeMap::from([("urn:fixture:schema:v1".to_owned(), 2)]),
            source_kind_totals: OptionalTextPartition {
                absent: 2,
                present: BTreeMap::new(),
            },
            source_state_totals: OptionalTextPartition {
                absent: 2,
                present: BTreeMap::new(),
            },
            terminal_class_totals: BTreeMap::from([("Selected".to_owned(), 2)]),
            policy_rule_totals: BTreeMap::from([("select".to_owned(), 2)]),
            resolution_totals: BTreeMap::from([
                (CensusResolution::Direct, 2),
                (CensusResolution::GraphOutput, 0),
                (CensusResolution::GraphNoOutput, 0),
            ]),
            funnel: CensusFunnel {
                decoded: 2,
                classified: 2,
                selected: 2,
                dependency_closed: 2,
                assembled: 2,
            },
            binding_closure: BindingClosureSummary {
                source_bindings: 0,
                corpus_bindings: 0,
            },
        }
    }

    fn receipt() -> ImportReceipt {
        ImportReceipt {
            schema: IMPORT_RECEIPT_SCHEMA.to_owned(),
            schema_version: IMPORT_RECEIPT_VERSION,
            receipt_id: "arbitrary-import-run".to_owned(),
            source_lock_digest: hash('1'),
            source_manifest_digest: hash('2'),
            corpus_manifest_digest: hash('3'),
            scope_policy_digest: hash('4'),
            importer_revision: "importer-rev-7".to_owned(),
            codec_revisions: BTreeMap::from([(
                "arbitrary-codec".to_owned(),
                "codec-rev-2".to_owned(),
            )]),
            source_identity_digest: hash('4'),
            state_digest: Some(hash('5')),
            outputs: vec![ReceiptOutputArtifact {
                logical_path: "generated/arbitrary-world.bin".to_owned(),
                sha256: hash('6'),
                bytes: 99,
            }],
            census: census(),
            validation_passed: true,
            authorization_receipt_digest: Some(hash('7')),
            attestation_digests: vec![hash('8')],
        }
    }

    #[test]
    fn receipt_round_trip_is_deterministic_and_binds_inputs() {
        let record = receipt();
        let bytes = record.to_json_bytes().unwrap();
        assert_eq!(record.to_json_bytes().unwrap(), bytes);
        assert_eq!(ImportReceipt::from_json(&bytes).unwrap(), record);
        assert!(record.binds(&hash('1'), &hash('2'), &hash('3'), &hash('4')));
        assert!(!record.binds(&hash('9'), &hash('2'), &hash('3'), &hash('4')));
        assert!(record.output("generated/arbitrary-world.bin").is_some());
    }

    #[test]
    fn tampered_binding_digest_is_detected() {
        let record = receipt();
        let mut tampered = record.clone();
        tampered.corpus_manifest_digest = hash('8');
        assert!(!record.binds(&hash('1'), &hash('2'), &hash('8'), &hash('4')));
        assert_ne!(
            record.to_json_bytes().unwrap(),
            tampered.to_json_bytes().unwrap()
        );
    }

    #[test]
    fn malformed_digests_and_revisions_are_rejected() {
        let mut zero_world = receipt();
        zero_world.source_identity_digest = hash('0');
        assert!(matches!(
            zero_world.validate(),
            Err(ImportReceiptError::FieldMismatch {
                field: "source_identity_digest",
                ..
            })
        ));

        let mut bad_state = receipt();
        bad_state.state_digest = Some("short".to_owned());
        assert!(matches!(
            bad_state.validate(),
            Err(ImportReceiptError::FieldMismatch {
                field: "state_digest",
                ..
            })
        ));

        let mut bad_codec = receipt();
        bad_codec
            .codec_revisions
            .insert("bad codec!".to_owned(), "rev-1".to_owned());
        assert!(matches!(
            bad_codec.validate(),
            Err(ImportReceiptError::FieldMismatch {
                field: "codec_revisions",
                ..
            })
        ));
    }

    #[test]
    fn duplicate_output_paths_and_unsorted_attestations_are_rejected() {
        let mut duplicate_output = receipt();
        duplicate_output
            .outputs
            .push(duplicate_output.outputs[0].clone());
        assert!(matches!(
            duplicate_output.validate(),
            Err(ImportReceiptError::FieldMismatch {
                field: "outputs",
                ..
            })
        ));

        let mut unsorted = receipt();
        unsorted.attestation_digests = vec![hash('9'), hash('8')];
        assert!(matches!(
            unsorted.validate(),
            Err(ImportReceiptError::FieldMismatch {
                field: "attestation_digests",
                ..
            })
        ));
    }

    #[test]
    fn census_summary_violations_surface_through_the_receipt() {
        let mut broken_census = receipt();
        broken_census.census.inventory_total = 5;
        assert!(matches!(
            broken_census.validate(),
            Err(ImportReceiptError::Census(
                CensusSummaryError::PartitionMismatch { .. }
            ))
        ));
    }

    #[test]
    fn zero_byte_output_with_exact_nonzero_digest_is_valid() {
        let mut record = receipt();
        record.outputs[0].bytes = 0;
        record.outputs[0].sha256 =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn zero_byte_output_rejects_nonempty_content_digest() {
        let mut record = receipt();
        record.outputs[0].bytes = 0;
        assert!(matches!(
            record.validate(),
            Err(ImportReceiptError::FieldMismatch {
                field: "outputs.sha256",
                ..
            })
        ));
    }

    #[test]
    fn promotion_equivalence_allows_only_receipt_and_governance_changes() {
        let candidate = receipt();
        let mut promoted = candidate.clone();
        promoted.receipt_id = "promoted-import-run".to_owned();
        promoted.authorization_receipt_digest = Some(hash('9'));
        promoted.attestation_digests = vec![hash('7')];
        assert!(require_promotion_equivalence(&candidate, &promoted).is_ok());
    }

    #[test]
    fn promotion_equivalence_rejects_substantive_result_drift() {
        let candidate = receipt();
        for (field, promoted) in [
            ("importer_revision", {
                let mut value = candidate.clone();
                value.importer_revision = "importer-rev-8".to_owned();
                value
            }),
            ("codec_revisions", {
                let mut value = candidate.clone();
                value
                    .codec_revisions
                    .insert("arbitrary-codec".to_owned(), "codec-rev-3".to_owned());
                value
            }),
            ("state_digest", {
                let mut value = candidate.clone();
                value.state_digest = Some(hash('9'));
                value
            }),
            ("outputs", {
                let mut value = candidate.clone();
                value.outputs[0].bytes += 1;
                value
            }),
            ("census", {
                let mut value = candidate.clone();
                value.census.source_schema_totals.clear();
                value
                    .census
                    .source_schema_totals
                    .insert("urn:fixture:schema:changed".to_owned(), 2);
                value
            }),
        ] {
            assert!(matches!(
                require_promotion_equivalence(&candidate, &promoted),
                Err(ReceiptPromotionError::ResultMismatch { field: observed })
                    if observed == field
            ));
        }
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let mut text = String::from_utf8(receipt().to_json_bytes().unwrap()).unwrap();
        text = text.replace("\"receipt_id\"", "\"unexpected_field\": 1, \"receipt_id\"");
        assert!(matches!(
            ImportReceipt::from_json(text.as_bytes()),
            Err(ImportReceiptError::Document(VersionedDocumentError::Json(
                _
            )))
        ));

        let text = String::from_utf8(receipt().to_json_bytes().unwrap())
            .unwrap()
            .replace("\"schema_version\":4", "\"schema_version\":5");
        assert!(matches!(
            ImportReceipt::from_json(text.as_bytes()),
            Err(ImportReceiptError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }
}
