//! An exact on-disk synthetic publication, verified through the public runtime API.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use worldrunner_world_model::{ContentDigest, SemanticWorldDocument};
use worldrunner_world_runtime::{
    PublicationLimits, PublicationPaths, VerifiedNormalPublication, VerifiedPublication,
    verify_publication,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub fn digest(bytes: &[u8]) -> ContentDigest {
    ContentDigest::from_bytes(Sha256::digest(bytes).into()).unwrap()
}

pub fn verify(
    world: &SemanticWorldDocument,
    contract_bytes: &[u8],
) -> (VerifiedNormalPublication, Directory) {
    let directory = Directory(std::env::temp_dir().join(format!(
        "render-payload-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&directory.0).unwrap();
    let mut outputs = Vec::new();
    for (path, bytes) in [
        ("contract.json", contract_bytes.to_vec()),
        ("left.bin", vec![3, 5]),
        ("right.bin", vec![7, 11]),
        ("world.json", serde_json::to_vec(world).unwrap()),
    ] {
        fs::write(directory.0.join(path), &bytes).unwrap();
        outputs.push(record(path, &bytes));
    }
    let normal = super::authority::verify(world, receipt(world, &outputs), |receipt| {
        publish(&directory, &outputs, &receipt)
    });
    (normal, directory)
}

fn publish(directory: &Directory, outputs: &[Value], receipt: &Value) -> VerifiedPublication {
    let bytes = serde_json::to_vec(receipt).unwrap();
    fs::write(directory.0.join("receipt.json"), &bytes).unwrap();
    let mut artifacts = outputs.to_vec();
    artifacts.push(record("receipt.json", &bytes));
    artifacts.sort_by(|left, right| {
        left["logical_path"]
            .as_str()
            .cmp(&right["logical_path"].as_str())
    });
    let descriptor = json!({ "schema": "real-disunity.publication-set.v1", "schema_version": 1,
        "publication_id": "synthetic-publication", "artifacts": artifacts });
    fs::write(
        directory.0.join("publication.json"),
        serde_json::to_vec(&descriptor).unwrap(),
    )
    .unwrap();
    verify_publication(
        &directory.0,
        PublicationPaths {
            descriptor: "publication.json",
            receipt: "receipt.json",
            world: "world.json",
        },
        PublicationLimits {
            max_artifacts: 16,
            max_artifact_bytes: 1_000_000,
            max_total_bytes: 2_000_000,
            max_path_bytes: 128,
            max_nodes: 32,
            max_depth: 4,
        },
        &super::support::validation_limits(),
    )
    .unwrap()
}

fn record(path: &str, bytes: &[u8]) -> Value {
    json!({ "logical_path": path, "sha256": digest(bytes).to_hex(), "bytes": bytes.len() })
}

fn receipt(world: &SemanticWorldDocument, outputs: &[Value]) -> Value {
    let authority = digest(b"synthetic-authority");
    json!({
        "schema": "real-disunity.import-receipt.v4", "schema_version": 4,
        "receipt_id": "synthetic-receipt", "source_lock_digest": authority.to_hex(),
        "source_manifest_digest": authority.to_hex(), "corpus_manifest_digest": authority.to_hex(),
        "scope_policy_digest": authority.to_hex(), "importer_revision": "synthetic-importer",
        "codec_revisions": {}, "source_identity_digest": world.source().digest().to_hex(),
        "state_digest": world.generated_state_digest(authority, authority).unwrap().to_hex(),
        "outputs": outputs, "validation_passed": true,
        "authorization_receipt_digest": null, "attestation_digests": [],
        "census": {
            "inventory_total": 0, "bucket_totals": {"selected": 0, "excluded_by_design": 0, "withheld": 0},
            "source_entry_totals": {}, "source_schema_totals": {},
            "source_kind_totals": {"absent": 0, "present": {}}, "source_state_totals": {"absent": 0, "present": {}},
            "terminal_class_totals": {}, "policy_rule_totals": {},
            "resolution_totals": {"direct": 0, "graph_output": 0, "graph_no_output": 0},
            "funnel": {"decoded": 0, "classified": 0, "selected": 0, "dependency_closed": 0, "assembled": 0},
            "binding_closure": {"source_bindings": 3, "corpus_bindings": 0}
        }
    })
}
