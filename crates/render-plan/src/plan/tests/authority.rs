//! Independent synthetic authority documents for public governed preparation tests.

use serde_json::{Value, json};
use worldrunner_world_model::SemanticWorldDocument;
use worldrunner_world_runtime::{
    NormalAuthorityDocuments, VerifiedNormalPublication, VerifiedPublication,
    verify_normal_authority,
};

use super::publication::digest;

pub fn verify(
    world: &SemanticWorldDocument,
    template: Value,
    publish: impl FnOnce(Value) -> VerifiedPublication,
) -> VerifiedNormalPublication {
    let authorities = NormalAuthorityDocuments {
        source_manifest: &serde_json::from_value(source_manifest()).unwrap(),
        source_lock: &serde_json::from_value(source_lock()).unwrap(),
        corpus_manifest: &serde_json::from_value(corpus()).unwrap(),
        scope_policy: &serde_json::from_value(policy()).unwrap(),
        candidate_receipt: &serde_json::from_value(template).unwrap(),
        attestation: &serde_json::from_value(attestation()).unwrap(),
    };
    let mut candidate = authorities.candidate_receipt.clone();
    candidate.source_manifest_digest =
        digest(&authorities.source_manifest.to_json_bytes().unwrap()).to_hex();
    candidate.source_lock_digest =
        digest(&authorities.source_lock.to_json_bytes().unwrap()).to_hex();
    let corpus = digest(&authorities.corpus_manifest.to_json_bytes().unwrap());
    let policy = digest(&authorities.scope_policy.to_json_bytes().unwrap());
    candidate.corpus_manifest_digest = corpus.to_hex();
    candidate.scope_policy_digest = policy.to_hex();
    candidate.state_digest = Some(
        world
            .generated_state_digest(corpus, policy)
            .unwrap()
            .to_hex(),
    );
    let candidate_digest = digest(&candidate.to_json_bytes().unwrap()).to_hex();
    let mut attestation = authorities.attestation.clone();
    attestation.subject_lock_digest = candidate.source_lock_digest.clone();
    attestation.subject_receipt_digest = candidate_digest.clone();
    let mut normal = candidate.clone();
    normal.receipt_id = "synthetic-normal".to_owned();
    normal.authorization_receipt_digest = Some(candidate_digest);
    normal.attestation_digests = vec![digest(&attestation.to_json_bytes().unwrap()).to_hex()];
    let publication = publish(serde_json::to_value(normal).unwrap());
    verify_normal_authority(
        publication,
        NormalAuthorityDocuments {
            candidate_receipt: &candidate,
            attestation: &attestation,
            ..authorities
        },
    )
    .unwrap()
}

fn source_manifest() -> Value {
    json!({"schema": "real-disunity.save-bundle-manifest.v1", "schema_version": 1,
        "manifest_id": "synthetic-source", "source_uri": "synthetic:source/alpha",
        "entries": [{"entry_id": "synthetic", "schema_uri": "synthetic:schema/source",
            "logical_path": "source.bin", "required": true, "descriptor": null}]})
}

fn source_lock() -> Value {
    let hash = digest(b"synthetic-source").to_hex();
    let identity = format!(
        r#"{{"schema":"real-disunity.source-observation-lock.v1","schema_version":1,"entries":[{{"entry_id":"synthetic","schema_uri":"synthetic:schema/source","logical_path":"source.bin","sha256":"{hash}","bytes":16,"structural_count":null}}]}}"#
    );
    let source = digest(identity.as_bytes()).to_hex();
    let mut lock: Value = serde_json::from_str(&identity).unwrap();
    lock["source_digest"] = json!(source);
    lock["lock_id"] = json!(format!("observed-{source}"));
    lock
}

fn corpus() -> Value {
    json!({"schema": "real-disunity.corpus-manifest.v2", "schema_version": 2,
        "manifest_id": "synthetic-corpus", "corpus_id": "synthetic-library",
        "artifacts": [{"logical_path": "library.bin", "sha256": digest(b"library").to_hex(), "bytes": 7}], "documents": []})
}

fn policy() -> Value {
    json!({"schema": "real-disunity.environment-scope-policy.v1", "schema_version": 1,
        "policy_id": "synthetic-policy", "agreed_exclusion_roots": [], "rules": []})
}

fn attestation() -> Value {
    let hash = digest(b"synthetic-evidence").to_hex();
    json!({"schema": "real-disunity.validation-attestation.v2", "schema_version": 2,
        "attestation_id": "synthetic-attestation", "subject_lock_digest": hash,
        "subject_receipt_digest": hash, "outcome": "approved", "reviewer": "synthetic-reviewer",
        "reason": "synthetic preparation fixture", "evidence": [{"evidence_id": "synthetic-evidence", "sha256": hash, "bytes": 18}]})
}
