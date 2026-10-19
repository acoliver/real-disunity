use crate::validation::{
    HashValidity, checked_sum, first_unsorted_unique, is_identifier, is_logical_path, is_uri,
    sha256_validity,
};
use crate::version::{
    SOURCE_OBSERVATION_LOCK_SCHEMA, SOURCE_OBSERVATION_LOCK_VERSION, VersionedDocumentError,
    parse_versioned, to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use thiserror::Error;

/// One immutably observed save-bundle artifact together with its content address.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedArtifactEntry {
    pub entry_id: String,
    pub schema_uri: String,
    pub logical_path: String,
    pub sha256: String,
    pub bytes: u64,
    pub structural_count: Option<u64>,
}

/// Immutable observed identity of one save bundle.
///
/// The lock records only what was observed about the bundle: its canonical identity, digest, and
/// the identity, schema, path, hash, size, and optional structural count of every artifact entry.
/// It deliberately contains no corpus, census, runtime, or world-construction data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceObservationLock {
    pub schema: String,
    pub schema_version: u32,
    pub lock_id: String,
    pub source_digest: String,
    pub entries: Vec<ObservedArtifactEntry>,
}

/// Identity projection that excludes lock bookkeeping such as `lock_id`.
#[derive(Serialize)]
struct SourceIdentityView<'a> {
    schema: &'a str,
    schema_version: u32,
    entries: &'a [ObservedArtifactEntry],
}

impl SourceObservationLock {
    /// Parses and validates a source-observation lock.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or lock validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SourceLockError> {
        let lock: Self = parse_versioned(
            bytes,
            SOURCE_OBSERVATION_LOCK_SCHEMA,
            SOURCE_OBSERVATION_LOCK_VERSION,
        )?;
        lock.validate()?;
        Ok(lock)
    }

    /// Derives a deterministic lock from observed entries.
    ///
    /// Entries may arrive in any order; they are sorted by `entry_id` before identity is
    /// derived. `source_digest` is the SHA-256 of the canonical identity bytes and `lock_id` is
    /// derived from that digest, so identical observations always produce identical locks.
    ///
    /// # Errors
    ///
    /// Returns an error when an entry violates the observation contract.
    pub fn from_observed_entries(
        mut entries: Vec<ObservedArtifactEntry>,
    ) -> Result<Self, SourceLockError> {
        entries.sort_by(|left, right| left.entry_id.cmp(&right.entry_id));
        let mut lock = Self {
            schema: SOURCE_OBSERVATION_LOCK_SCHEMA.to_owned(),
            schema_version: SOURCE_OBSERVATION_LOCK_VERSION,
            lock_id: String::new(),
            source_digest: String::new(),
            entries,
        };
        lock.source_digest = lock.derived_source_digest()?;
        lock.lock_id = derived_lock_id(&lock.source_digest);
        lock.validate()?;
        Ok(lock)
    }

    /// Serializes this lock as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, SourceLockError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this lock as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, SourceLockError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Returns the canonical bytes that define this save bundle's identity.
    ///
    /// Callers hash these bytes to verify the stored `source_digest`. Lock bookkeeping fields
    /// are excluded so the identity depends only on the observed source content.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock is invalid or serialization fails.
    pub fn source_identity_bytes(&self) -> Result<Vec<u8>, SourceLockError> {
        self.validate()?;
        self.identity_view_bytes()
    }

    /// Serializes the identity view without validating, so derivation cannot recurse.
    fn identity_view_bytes(&self) -> Result<Vec<u8>, SourceLockError> {
        let view = SourceIdentityView {
            schema: &self.schema,
            schema_version: self.schema_version,
            entries: &self.entries,
        };
        Ok(to_canonical_vec(&view)?)
    }

    /// Hashes the canonical identity bytes into the observed source digest.
    fn derived_source_digest(&self) -> Result<String, SourceLockError> {
        let identity = self.identity_view_bytes()?;
        Ok(sha256_hex(&identity))
    }

    /// Rejects stored bookkeeping that disagrees with the derived identity.
    fn validate_derived_identity(&self) -> Result<(), SourceLockError> {
        let derived_digest = self.derived_source_digest()?;
        if derived_digest != self.source_digest {
            return Err(invalid_field(
                "source_digest",
                &derived_digest,
                &self.source_digest,
            ));
        }
        let expected_lock_id = derived_lock_id(&derived_digest);
        if expected_lock_id != self.lock_id {
            return Err(invalid_field("lock_id", &expected_lock_id, &self.lock_id));
        }
        Ok(())
    }

    /// Finds an observed entry by its stable entry identifier.
    #[must_use]
    pub fn entry(&self, entry_id: &str) -> Option<&ObservedArtifactEntry> {
        self.entries
            .binary_search_by_key(&entry_id, |entry| entry.entry_id.as_str())
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Validates the envelope, identities, digests, paths, counts, and total size.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed observation violation.
    pub fn validate(&self) -> Result<(), SourceLockError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            SOURCE_OBSERVATION_LOCK_SCHEMA,
            SOURCE_OBSERVATION_LOCK_VERSION,
        )?;
        if !is_identifier(&self.lock_id) {
            return Err(invalid_field(
                "lock_id",
                "nonempty identifier",
                &self.lock_id,
            ));
        }
        validate_hash("source_digest", &self.source_digest)?;
        validate_entries(&self.entries)?;
        self.validate_derived_identity()
    }
}

/// Derives the deterministic lock identifier from an observed source digest.
fn derived_lock_id(source_digest: &str) -> String {
    format!("observed-{source_digest}")
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

fn validate_entries(entries: &[ObservedArtifactEntry]) -> Result<(), SourceLockError> {
    if entries.is_empty() {
        return Err(invalid_field(
            "entries",
            "at least one observed entry",
            "empty",
        ));
    }
    let entry_ids: Vec<String> = entries.iter().map(|entry| entry.entry_id.clone()).collect();
    if let Some(index) = first_unsorted_unique(&entry_ids) {
        return Err(invalid_field(
            "entries.entry_id",
            "strictly sorted unique identifiers",
            &entries[index].entry_id,
        ));
    }
    validate_entry_uniqueness(entries)?;
    for entry in entries {
        validate_entry(entry)?;
    }
    if checked_sum(entries.iter().map(|entry| entry.bytes)).is_none() {
        return Err(invalid_field(
            "entries.bytes",
            "non-overflowing total size",
            "overflow",
        ));
    }
    Ok(())
}

fn validate_entry_uniqueness(entries: &[ObservedArtifactEntry]) -> Result<(), SourceLockError> {
    let mut paths = std::collections::BTreeSet::new();
    for entry in entries {
        if !paths.insert(entry.logical_path.as_str()) {
            return Err(invalid_field(
                "entries.logical_path",
                "unique logical path",
                &entry.logical_path,
            ));
        }
    }
    Ok(())
}

fn validate_entry(entry: &ObservedArtifactEntry) -> Result<(), SourceLockError> {
    if !is_identifier(&entry.entry_id) {
        return Err(invalid_field(
            "entries.entry_id",
            "nonempty identifier",
            &entry.entry_id,
        ));
    }
    if !is_uri(&entry.schema_uri) {
        return Err(invalid_field(
            "entries.schema_uri",
            "schema URI",
            &entry.schema_uri,
        ));
    }
    if !is_logical_path(&entry.logical_path) {
        return Err(invalid_field(
            "entries.logical_path",
            "normalized relative logical path",
            &entry.logical_path,
        ));
    }
    validate_hash("entries.sha256", &entry.sha256)?;
    Ok(())
}

fn validate_hash(field: &'static str, hash: &str) -> Result<(), SourceLockError> {
    let expected = match sha256_validity(hash) {
        HashValidity::Valid => return Ok(()),
        HashValidity::Invalid => "64 hexadecimal SHA-256 digits",
        HashValidity::AllZero => "nonzero SHA-256",
    };
    Err(invalid_field(field, expected, hash))
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> SourceLockError {
    SourceLockError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Source-lock decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum SourceLockError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("source lock field {field} mismatch: expected {expected}, observed {observed}")]
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
    use std::collections::BTreeSet;

    fn hash(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn entry(entry_id: &str, digit: char) -> ObservedArtifactEntry {
        ObservedArtifactEntry {
            entry_id: entry_id.to_owned(),
            schema_uri: "urn:real-disunity:artifact:opaque".to_owned(),
            logical_path: format!("observed/{entry_id}.bin"),
            sha256: hash(digit),
            bytes: 16,
            structural_count: Some(4),
        }
    }

    /// Deliberately unsorted: the constructor must canonicalize entry ordering.
    fn observed_entries() -> Vec<ObservedArtifactEntry> {
        vec![entry("beta-stream", '2'), entry("alpha-stream", '1')]
    }

    fn source_lock() -> SourceObservationLock {
        SourceObservationLock::from_observed_entries(observed_entries()).unwrap()
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
    fn constructor_sorts_entries_and_derives_deterministic_identity() {
        let lock = source_lock();
        assert_eq!(
            lock.entries
                .iter()
                .map(|e| e.entry_id.as_str())
                .collect::<Vec<_>>(),
            ["alpha-stream", "beta-stream"]
        );
        assert_eq!(
            lock.source_digest,
            sha256_hex(&lock.source_identity_bytes().unwrap())
        );
        assert_eq!(lock.lock_id, format!("observed-{}", lock.source_digest));

        let reordered = SourceObservationLock::from_observed_entries(vec![
            entry("alpha-stream", '1'),
            entry("beta-stream", '2'),
        ])
        .unwrap();
        assert_eq!(lock, reordered);
    }

    #[test]
    fn lock_round_trip_is_deterministic() {
        let lock = source_lock();
        let bytes = lock.to_json_bytes().unwrap();
        assert_eq!(lock.to_json_bytes().unwrap(), bytes);
        assert_eq!(SourceObservationLock::from_json(&bytes).unwrap(), lock);
        assert_eq!(lock.entry("beta-stream").map(|e| e.bytes), Some(16));
    }

    #[test]
    fn changed_artifact_observation_changes_derived_digest() {
        let original = source_lock();
        let mut drifted_entry = entry("beta-stream", '9');
        drifted_entry.bytes = 48;
        let drifted = SourceObservationLock::from_observed_entries(vec![
            entry("alpha-stream", '1'),
            drifted_entry,
        ])
        .unwrap();
        assert_ne!(original.source_digest, drifted.source_digest);
        assert_ne!(original.lock_id, drifted.lock_id);
    }

    #[test]
    fn distinct_observations_have_distinct_identities() {
        let other = SourceObservationLock::from_observed_entries(vec![
            entry("alpha-stream", '1'),
            entry("gamma-stream", '3'),
        ])
        .unwrap();
        assert_ne!(
            source_lock().source_identity_bytes().unwrap(),
            other.source_identity_bytes().unwrap()
        );
    }

    #[test]
    fn derived_bookkeeping_stays_outside_source_identity() {
        let lock = source_lock();
        let view: Value = serde_json::from_slice(&lock.source_identity_bytes().unwrap()).unwrap();
        let keys: Vec<&str> = view
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["entries", "schema", "schema_version"]);
    }

    #[test]
    fn inconsistent_derived_digest_and_lock_id_are_rejected() {
        let mut drifted_digest = source_lock();
        drifted_digest.source_digest = hash('b');
        match drifted_digest.validate() {
            Err(SourceLockError::FieldMismatch {
                field: "source_digest",
                expected,
                observed,
            }) => {
                assert_eq!(expected, source_lock().source_digest);
                assert_eq!(observed, hash('b'));
            }
            other => panic!("expected source digest mismatch, observed {other:?}"),
        }

        let mut drifted_lock_id = source_lock();
        drifted_lock_id.lock_id = "observed-mismatched".to_owned();
        assert!(matches!(
            drifted_lock_id.validate(),
            Err(SourceLockError::FieldMismatch {
                field: "lock_id",
                ..
            })
        ));
    }

    #[test]
    fn lock_identity_is_independent_of_corpus_and_runtime_concerns() {
        let value: Value = serde_json::from_slice(&source_lock().to_json_bytes().unwrap()).unwrap();
        for concept in ["corpus", "census", "runtime", "world", "profile"] {
            assert!(
                !key_tree_contains(&value, concept),
                "lock must not mention {concept}"
            );
        }
    }

    #[test]
    fn duplicate_ids_unsafe_paths_and_bad_hashes_are_rejected() {
        let duplicate = SourceObservationLock::from_observed_entries(vec![
            entry("alpha-stream", '1'),
            entry("alpha-stream", '3'),
        ]);
        assert!(matches!(
            duplicate,
            Err(SourceLockError::FieldMismatch {
                field: "entries.entry_id",
                ..
            })
        ));

        let mut unsafe_path = source_lock();
        unsafe_path.entries[0].logical_path = "../escape.bin".to_owned();
        assert!(matches!(
            unsafe_path.validate(),
            Err(SourceLockError::FieldMismatch {
                field: "entries.logical_path",
                ..
            })
        ));

        let mut zero_hash = source_lock();
        zero_hash.entries[0].sha256 = hash('0');
        assert!(matches!(
            zero_hash.validate(),
            Err(SourceLockError::FieldMismatch {
                field: "entries.sha256",
                ..
            })
        ));
    }

    #[test]
    fn empty_artifacts_and_zero_structural_counts_are_valid() {
        let mut empty_entry = entry("alpha-stream", '1');
        empty_entry.bytes = 0;
        empty_entry.structural_count = Some(0);
        let lock = SourceObservationLock::from_observed_entries(vec![
            empty_entry,
            entry("beta-stream", '2'),
        ])
        .unwrap();
        assert!(lock.validate().is_ok());
        assert_eq!(lock.entry("alpha-stream").map(|e| e.bytes), Some(0));
    }

    #[test]
    fn overflowing_total_sizes_are_still_rejected() {
        let mut overflow = entry("alpha-stream", '1');
        overflow.bytes = u64::MAX;
        let lock =
            SourceObservationLock::from_observed_entries(vec![overflow, entry("beta-stream", '2')]);
        assert!(matches!(
            lock,
            Err(SourceLockError::FieldMismatch {
                field: "entries.bytes",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let mut text = String::from_utf8(source_lock().to_json_bytes().unwrap()).unwrap();
        text = text.replace("\"lock_id\"", "\"unexpected_field\": 1, \"lock_id\"");
        assert!(matches!(
            SourceObservationLock::from_json(text.as_bytes()),
            Err(SourceLockError::Document(VersionedDocumentError::Json(_)))
        ));

        let text = String::from_utf8(source_lock().to_json_bytes().unwrap())
            .unwrap()
            .replace("\"schema_version\":1", "\"schema_version\":7");
        assert!(matches!(
            SourceObservationLock::from_json(text.as_bytes()),
            Err(SourceLockError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }

    #[test]
    fn arbitrary_entry_counts_and_identifiers_are_supported() {
        let count = 40_usize;
        let entries: Vec<ObservedArtifactEntry> = (0..count)
            .map(|index| {
                let mut observed = entry(&format!("stream-{index:04}"), '1');
                observed.logical_path = format!("observed/stream-{index:04}.bin");
                observed.bytes = (index % 7) as u64;
                observed
            })
            .collect();
        let lock = SourceObservationLock::from_observed_entries(entries).unwrap();
        assert_eq!(lock.entries.len(), count);
        assert!(
            lock.entries
                .windows(2)
                .all(|pair| pair[0].entry_id < pair[1].entry_id)
        );
        let mut ids = BTreeSet::new();
        assert!(lock.entries.iter().all(|e| ids.insert(e.entry_id.clone())));
    }
}
