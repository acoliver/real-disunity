use super::support;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use worldrunner_import_schema::{
    BindingClosureSummary, CensusBucket, CensusFunnel, CensusResolution, CensusSummary,
    IMPORT_RECEIPT_SCHEMA, IMPORT_RECEIPT_VERSION, ImportReceipt, OptionalTextPartition,
    PUBLICATION_SET_SCHEMA, PUBLICATION_SET_VERSION, PublicationSetDescriptor, PublishedArtifact,
    ReceiptOutputArtifact,
};
use worldrunner_world_model::{
    ContentDigest, DecodedSave, ResourceBinding, SchemaDescriptor, SemanticComponent, SemanticMap,
    SemanticWorldDocument, SourceIdentity, ValidationLimits,
};
use worldrunner_world_runtime::{
    BoundResourceReadError, PublicationLimits, PublicationPaths, PublicationVerificationError,
    verify_publication,
};

const DESCRIPTOR_PATH: &str = "publication.json";
const RECEIPT_PATH: &str = "evidence/receipt.json";
const WORLD_PATH: &str = "world.json";
const CORPUS_DIGEST: [u8; 32] = [0x43; 32];
const POLICY_DIGEST: [u8; 32] = [0x50; 32];
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
const RESOURCE_BYTES: &[u8] = b"bound-resource";

struct Fixture {
    root: PathBuf,
    world: SemanticWorldDocument,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn exact_publication_returns_canonical_root_and_verified_identities() {
    let fixture = fixture(None);
    let verified = verify_publication(&fixture.root, paths(), limits(), &world_limits())
        .expect("valid publication");

    assert_eq!(verified.root(), fixture.root.canonicalize().unwrap());
    assert_eq!(verified.world(), &fixture.world);
    assert_eq!(
        verified.receipt().source_identity_digest,
        fixture.world.source().digest().to_hex()
    );
    assert_eq!(
        verified.generated_state_digest().to_hex(),
        generated_state(&fixture.world)
    );
}

#[test]
fn stale_generated_state_fails_even_when_filesystem_hashes_are_self_consistent() {
    let fixture = fixture(Some(hash_byte(0x71)));
    let error = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap_err();

    assert!(matches!(
        error,
        PublicationVerificationError::IdentityMismatch {
            field: "generated_state",
            ..
        }
    ));
}

#[test]
fn added_files_fail_exact_publication_closure() {
    let fixture = fixture(None);
    fs::write(fixture.root.join("added.bin"), b"unbound").unwrap();
    let error = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap_err();

    assert!(matches!(
        error,
        PublicationVerificationError::UnexpectedNode { path } if path == "added.bin"
    ));
}

#[test]
fn publication_traversal_node_limit_fails_closed() {
    let fixture = fixture(None);
    let mut bounded = limits();
    bounded.max_nodes = 3;
    let error = verify_publication(&fixture.root, paths(), bounded, &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::LimitExceeded {
            limit: "filesystem node count",
            observed: 4,
            maximum: 3,
        }
    ));
}

#[test]
fn publication_traversal_depth_limit_fails_closed() {
    let fixture = fixture(None);
    let mut bounded = limits();
    bounded.max_depth = 1;
    let error = verify_publication(&fixture.root, paths(), bounded, &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::LimitExceeded {
            limit: "filesystem depth",
            observed: 2,
            maximum: 1,
        }
    ));
}

#[test]
fn declared_total_byte_limit_fails_before_artifact_parsing() {
    let fixture = fixture(None);
    let mut bounded = limits();
    bounded.max_total_bytes = fs::metadata(fixture.root.join(DESCRIPTOR_PATH))
        .unwrap()
        .len()
        .saturating_add(1);
    let error = verify_publication(&fixture.root, paths(), bounded, &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::LimitExceeded {
            limit: "total byte length",
            ..
        }
    ));
}

#[test]
fn underdeclared_receipt_fails_before_document_parsing() {
    let fixture = fixture(None);
    let descriptor_path = fixture.root.join(DESCRIPTOR_PATH);
    let mut descriptor =
        PublicationSetDescriptor::from_json(&fs::read(&descriptor_path).unwrap()).unwrap();
    descriptor
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.logical_path == RECEIPT_PATH)
        .unwrap()
        .bytes = 1;
    fs::write(descriptor_path, descriptor.to_json_bytes().unwrap()).unwrap();

    let error = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::LimitExceeded {
            limit: "artifact byte length",
            maximum: 1,
            ..
        }
    ));
}
#[test]
fn bound_member_reopens_and_planner_detects_post_verification_tampering() {
    use worldrunner_world_runtime::{ContentAddress, ResourceProvider, RuntimePlanError};

    let fixture = resource_fixture(&["resources/bound.bin"]);
    let verified = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap();
    let address = ContentAddress::from(&verified.world().bindings()[0]);
    assert_eq!(
        verified.resource_member(address),
        Some("resources/bound.bin")
    );
    assert!(verified.open(support::content_address(b"unknown")).is_err());
    support::prepare(&support::standard_registry(), verified.world(), &verified).unwrap();

    fs::write(fixture.root.join("resources/bound.bin"), b"xound-resource").unwrap();
    let error =
        support::prepare(&support::standard_registry(), verified.world(), &verified).unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::ResourceDigestMismatch { .. }
    ));
}

#[test]
fn bounded_binding_read_requires_the_exact_retained_binding_and_rechecks_bytes() {
    let fixture = resource_fixture(&["resources/bound.bin"]);
    let verified = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap();
    let binding = &verified.world().bindings()[0];

    assert_eq!(
        verified
            .read_binding_bounded(binding, u64::try_from(RESOURCE_BYTES.len()).unwrap())
            .unwrap(),
        RESOURCE_BYTES
    );
    assert!(matches!(
        verified.read_binding_bounded(binding, 1).unwrap_err(),
        BoundResourceReadError::LimitExceeded { .. }
    ));

    let mut drifted = binding.clone();
    drifted.capability_uri = "test:capability:drifted:v1".to_owned();
    assert!(matches!(
        verified
            .read_binding_bounded(&drifted, u64::MAX)
            .unwrap_err(),
        BoundResourceReadError::BindingMismatch { .. }
    ));
    let foreign = ResourceBinding {
        binding_id: "binding-foreign".to_owned(),
        ..binding.clone()
    };
    assert!(matches!(
        verified
            .read_binding_bounded(&foreign, u64::MAX)
            .unwrap_err(),
        BoundResourceReadError::UnknownBinding { .. }
    ));

    fs::write(fixture.root.join("resources/bound.bin"), b"xound-resource").unwrap();
    assert!(matches!(
        verified
            .read_binding_bounded(binding, u64::MAX)
            .unwrap_err(),
        BoundResourceReadError::DigestMismatch { .. }
    ));
}

#[test]
fn bounded_binding_read_accepts_zero_and_fixed_chunk_boundaries() {
    for bytes in [Vec::new(), vec![0x51; 65_536], vec![0x73; 131_072]] {
        let fixture = resource_fixture_with_bytes(&["resources/bound.bin"], &bytes);
        let verified =
            verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap();
        let binding = &verified.world().bindings()[0];
        assert_eq!(
            verified
                .read_binding_bounded(binding, u64::try_from(bytes.len()).unwrap())
                .unwrap(),
            bytes
        );
    }
}

#[test]
fn bounded_binding_read_rejects_short_and_long_post_verification_streams() {
    let expected = vec![0x39; 65_536];

    let short_fixture = resource_fixture_with_bytes(&["resources/bound.bin"], &expected);
    let short_verified =
        verify_publication(&short_fixture.root, paths(), limits(), &world_limits()).unwrap();
    let short_binding = &short_verified.world().bindings()[0];
    fs::write(
        short_fixture.root.join("resources/bound.bin"),
        &expected[..expected.len() - 1],
    )
    .unwrap();
    assert!(matches!(
        short_verified
            .read_binding_bounded(short_binding, u64::MAX)
            .unwrap_err(),
        BoundResourceReadError::ByteLengthMismatch { .. }
    ));

    let long_fixture = resource_fixture_with_bytes(&["resources/bound.bin"], &expected);
    let long_verified =
        verify_publication(&long_fixture.root, paths(), limits(), &world_limits()).unwrap();
    let long_binding = &long_verified.world().bindings()[0];
    let mut longer = expected;
    longer.push(0x39);
    fs::write(long_fixture.root.join("resources/bound.bin"), longer).unwrap();
    assert!(matches!(
        long_verified
            .read_binding_bounded(long_binding, u64::MAX)
            .unwrap_err(),
        BoundResourceReadError::ByteLengthMismatch { .. }
    ));
}

#[test]
fn bounded_binding_read_does_not_open_unrelated_resources() {
    let fixture = two_resource_fixture();
    let verified = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap();
    let selected = verified
        .world()
        .bindings()
        .iter()
        .find(|binding| binding.binding_id == "binding-alpha")
        .unwrap();
    fs::remove_file(fixture.root.join("resources/beta.bin")).unwrap();

    assert_eq!(
        verified.read_binding_bounded(selected, u64::MAX).unwrap(),
        RESOURCE_BYTES
    );
}

#[test]
fn receipt_binding_closure_must_equal_the_verified_world() {
    let fixture = resource_fixture(&["resources/bound.bin"]);
    rewrite_binding_closure(&fixture.root, 0);

    let error = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::BindingClosureMismatch {
            receipt: 0,
            world: 1
        }
    ));
}

#[test]
fn missing_bound_resource_member_fails_closed() {
    let fixture = resource_fixture(&[]);
    let error = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::MissingResourceMember { .. }
    ));
}

#[test]
fn duplicate_members_for_one_bound_address_are_ambiguous() {
    let fixture = resource_fixture(&["resources/first.bin", "resources/second.bin"]);
    let error = verify_publication(&fixture.root, paths(), limits(), &world_limits()).unwrap_err();
    assert!(matches!(
        error,
        PublicationVerificationError::AmbiguousResourceMember { .. }
    ));
}

fn fixture(state_override: Option<String>) -> Fixture {
    let root = scratch_directory();
    fs::create_dir_all(root.join("evidence")).unwrap();
    let world = world();
    let world_bytes = serde_json::to_vec(&world).unwrap();
    fs::write(root.join(WORLD_PATH), &world_bytes).unwrap();
    let world_output = output(WORLD_PATH, &world_bytes);
    let receipt = receipt(&world, vec![world_output.clone()], state_override);
    let receipt_bytes = receipt.to_json_bytes().unwrap();
    fs::write(root.join(RECEIPT_PATH), &receipt_bytes).unwrap();
    let descriptor = PublicationSetDescriptor {
        schema: PUBLICATION_SET_SCHEMA.to_owned(),
        schema_version: PUBLICATION_SET_VERSION,
        publication_id: "publication-test".to_owned(),
        artifacts: vec![
            published(RECEIPT_PATH, &receipt_bytes),
            PublishedArtifact {
                logical_path: world_output.logical_path,
                sha256: world_output.sha256,
                bytes: world_output.bytes,
            },
        ],
    };
    fs::write(
        root.join(DESCRIPTOR_PATH),
        descriptor.to_json_bytes().unwrap(),
    )
    .unwrap();
    Fixture { root, world }
}

fn resource_fixture(resource_paths: &[&str]) -> Fixture {
    resource_fixture_with_bytes(resource_paths, RESOURCE_BYTES)
}

fn resource_fixture_with_bytes(resource_paths: &[&str], resource_bytes: &[u8]) -> Fixture {
    let root = scratch_directory();
    fs::create_dir_all(root.join("evidence")).unwrap();
    let world = support::document(
        vec![support::component(
            "component-alpha",
            support::ALPHA_SCHEMA,
            support::alpha_payload("binding-alpha"),
        )],
        vec![support::binding(
            "binding-alpha",
            support::BYTES_CAPABILITY,
            resource_bytes,
            None,
        )],
    );
    let world_bytes = serde_json::to_vec(&world).unwrap();
    fs::write(root.join(WORLD_PATH), &world_bytes).unwrap();
    let mut outputs = Vec::new();
    for path in resource_paths {
        let absolute = root.join(path);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(&absolute, resource_bytes).unwrap();
        outputs.push(output(path, resource_bytes));
    }
    outputs.push(output(WORLD_PATH, &world_bytes));
    let receipt = receipt(&world, outputs.clone(), None);
    let receipt_bytes = receipt.to_json_bytes().unwrap();
    fs::write(root.join(RECEIPT_PATH), &receipt_bytes).unwrap();
    let mut artifacts = vec![published(RECEIPT_PATH, &receipt_bytes)];
    artifacts.extend(outputs.into_iter().map(|output| PublishedArtifact {
        logical_path: output.logical_path,
        sha256: output.sha256,
        bytes: output.bytes,
    }));
    let descriptor = PublicationSetDescriptor {
        schema: PUBLICATION_SET_SCHEMA.to_owned(),
        schema_version: PUBLICATION_SET_VERSION,
        publication_id: "publication-resource-test".to_owned(),
        artifacts,
    };
    fs::write(
        root.join(DESCRIPTOR_PATH),
        descriptor.to_json_bytes().unwrap(),
    )
    .unwrap();
    Fixture { root, world }
}

fn two_resource_fixture() -> Fixture {
    const BETA_BYTES: &[u8] = b"unrelated-resource";
    let root = scratch_directory();
    fs::create_dir_all(root.join("evidence")).unwrap();
    fs::create_dir_all(root.join("resources")).unwrap();
    let world = support::document(
        vec![
            support::component(
                "component-alpha",
                support::ALPHA_SCHEMA,
                support::alpha_payload("binding-alpha"),
            ),
            support::component(
                "component-beta",
                support::BETA_SCHEMA,
                support::beta_payload("binding-beta"),
            ),
        ],
        vec![
            support::binding(
                "binding-alpha",
                support::BYTES_CAPABILITY,
                RESOURCE_BYTES,
                None,
            ),
            support::binding("binding-beta", support::BYTES_CAPABILITY, BETA_BYTES, None),
        ],
    );
    let world_bytes = serde_json::to_vec(&world).unwrap();
    fs::write(root.join(WORLD_PATH), &world_bytes).unwrap();
    fs::write(root.join("resources/alpha.bin"), RESOURCE_BYTES).unwrap();
    fs::write(root.join("resources/beta.bin"), BETA_BYTES).unwrap();
    let outputs = vec![
        output("resources/alpha.bin", RESOURCE_BYTES),
        output("resources/beta.bin", BETA_BYTES),
        output(WORLD_PATH, &world_bytes),
    ];
    let receipt = receipt(&world, outputs.clone(), None);
    let receipt_bytes = receipt.to_json_bytes().unwrap();
    fs::write(root.join(RECEIPT_PATH), &receipt_bytes).unwrap();
    let mut artifacts = vec![published(RECEIPT_PATH, &receipt_bytes)];
    artifacts.extend(outputs.into_iter().map(|output| PublishedArtifact {
        logical_path: output.logical_path,
        sha256: output.sha256,
        bytes: output.bytes,
    }));
    let descriptor = PublicationSetDescriptor {
        schema: PUBLICATION_SET_SCHEMA.to_owned(),
        schema_version: PUBLICATION_SET_VERSION,
        publication_id: "publication-two-resource-test".to_owned(),
        artifacts,
    };
    fs::write(
        root.join(DESCRIPTOR_PATH),
        descriptor.to_json_bytes().unwrap(),
    )
    .unwrap();
    Fixture { root, world }
}

fn rewrite_binding_closure(root: &std::path::Path, source_bindings: u64) {
    let receipt_path = root.join(RECEIPT_PATH);
    let mut receipt = ImportReceipt::from_json(&fs::read(&receipt_path).unwrap()).unwrap();
    receipt.census.binding_closure = BindingClosureSummary {
        source_bindings,
        corpus_bindings: 0,
    };
    let receipt_bytes = receipt.to_json_bytes().unwrap();
    fs::write(&receipt_path, &receipt_bytes).unwrap();

    let descriptor_path = root.join(DESCRIPTOR_PATH);
    let mut descriptor =
        PublicationSetDescriptor::from_json(&fs::read(&descriptor_path).unwrap()).unwrap();
    let artifact = descriptor
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.logical_path == RECEIPT_PATH)
        .unwrap();
    artifact.sha256 = sha256_hex(&receipt_bytes);
    artifact.bytes = u64::try_from(receipt_bytes.len()).unwrap();
    fs::write(descriptor_path, descriptor.to_json_bytes().unwrap()).unwrap();
}

fn world() -> SemanticWorldDocument {
    let decoded = DecodedSave::new(
        vec![SemanticComponent {
            component_id: "component".to_owned(),
            schema_uri: "test:component:v1".to_owned(),
            payload: SemanticMap::default(),
        }],
        &world_limits(),
    )
    .unwrap();
    let source = SourceIdentity::of(&decoded).unwrap();
    SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "test:world:v1".to_owned(),
            version: 1,
        },
        source,
        decoded.components().to_vec(),
        Vec::new(),
        &world_limits(),
    )
    .unwrap()
}

fn receipt(
    world: &SemanticWorldDocument,
    outputs: Vec<ReceiptOutputArtifact>,
    state_override: Option<String>,
) -> ImportReceipt {
    let mut census = empty_census();
    census.binding_closure.source_bindings = u64::try_from(world.bindings().len()).unwrap();
    ImportReceipt {
        schema: IMPORT_RECEIPT_SCHEMA.to_owned(),
        schema_version: IMPORT_RECEIPT_VERSION,
        receipt_id: "receipt-test".to_owned(),
        source_lock_digest: hash_byte(0x11),
        source_manifest_digest: hash_byte(0x22),
        corpus_manifest_digest: hash_hex(CORPUS_DIGEST),
        scope_policy_digest: hash_hex(POLICY_DIGEST),
        importer_revision: "test-revision".to_owned(),
        codec_revisions: BTreeMap::new(),
        source_identity_digest: world.source().digest().to_hex(),
        state_digest: Some(state_override.unwrap_or_else(|| generated_state(world))),
        outputs,
        census,
        validation_passed: true,
        authorization_receipt_digest: None,
        attestation_digests: Vec::new(),
    }
}

fn empty_census() -> CensusSummary {
    CensusSummary {
        inventory_total: 0,
        bucket_totals: [
            CensusBucket::Selected,
            CensusBucket::ExcludedByDesign,
            CensusBucket::Withheld,
        ]
        .into_iter()
        .map(|bucket| (bucket, 0))
        .collect(),
        source_entry_totals: BTreeMap::new(),
        source_schema_totals: BTreeMap::new(),
        source_kind_totals: OptionalTextPartition {
            absent: 0,
            present: BTreeMap::new(),
        },
        source_state_totals: OptionalTextPartition {
            absent: 0,
            present: BTreeMap::new(),
        },
        terminal_class_totals: BTreeMap::new(),
        policy_rule_totals: BTreeMap::new(),
        resolution_totals: [
            CensusResolution::Direct,
            CensusResolution::GraphOutput,
            CensusResolution::GraphNoOutput,
        ]
        .into_iter()
        .map(|resolution| (resolution, 0))
        .collect(),
        funnel: CensusFunnel {
            decoded: 0,
            classified: 0,
            selected: 0,
            dependency_closed: 0,
            assembled: 0,
        },
        binding_closure: BindingClosureSummary {
            source_bindings: 0,
            corpus_bindings: 0,
        },
    }
}

fn generated_state(world: &SemanticWorldDocument) -> String {
    world
        .generated_state_digest(
            ContentDigest::from_bytes(CORPUS_DIGEST).unwrap(),
            ContentDigest::from_bytes(POLICY_DIGEST).unwrap(),
        )
        .unwrap()
        .to_hex()
}

fn output(path: &str, bytes: &[u8]) -> ReceiptOutputArtifact {
    ReceiptOutputArtifact {
        logical_path: path.to_owned(),
        sha256: sha256_hex(bytes),
        bytes: u64::try_from(bytes.len()).unwrap(),
    }
}

fn published(path: &str, bytes: &[u8]) -> PublishedArtifact {
    PublishedArtifact {
        logical_path: path.to_owned(),
        sha256: sha256_hex(bytes),
        bytes: u64::try_from(bytes.len()).unwrap(),
    }
}

fn paths() -> PublicationPaths<'static> {
    PublicationPaths {
        descriptor: DESCRIPTOR_PATH,
        receipt: RECEIPT_PATH,
        world: WORLD_PATH,
    }
}

fn limits() -> PublicationLimits {
    PublicationLimits {
        max_artifacts: 8,
        max_artifact_bytes: 1_000_000,
        max_total_bytes: 2_000_000,
        max_path_bytes: 128,
        max_nodes: 16,
        max_depth: 4,
    }
}

fn world_limits() -> ValidationLimits {
    ValidationLimits {
        max_depth: 8,
        max_nodes: 32,
        max_text_bytes: 256,
    }
}

fn scratch_directory() -> PathBuf {
    let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "real-disunity-world-runtime-publication-{}-{sequence}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir(&root).unwrap();
    root
}

fn hash_byte(byte: u8) -> String {
    hash_hex([byte; 32])
}

fn hash_hex(bytes: [u8; 32]) -> String {
    encode_hex(&bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    encode_hex(&Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").unwrap();
            output
        },
    )
}
