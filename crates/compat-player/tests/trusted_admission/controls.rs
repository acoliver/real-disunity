use super::{Boundary, CurrentInputs, Fixture, Recorder};
use crate::fixture::{SETTINGS, inventory};
use std::fs;
use worldrunner::compat_content::{ResourceKind, TrustedProviderInventory};

#[test]
fn stale_project_bytes_reject_before_any_protocol_request_or_queued_effect() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let inventory = Fixture::inventory();
    let source = Fixture::source(&receipt, 0);
    let boundary = Boundary::prepare(
        fixture.root(),
        &translated,
        Fixture::inventory(),
        Fixture::source(&receipt, 0),
    )
    .unwrap();
    let bytes = fs::read_to_string(fixture.root().join(SETTINGS)).unwrap();
    fs::write(
        fixture.root().join(SETTINGS),
        bytes.replace("fileID: 7", "fileID: 8"),
    )
    .unwrap();
    let current = CurrentInputs::new(fixture.root(), &policy, &inventory, &source);
    let mut recorder = Recorder::default();
    recorder.queue("retained-old-input");
    let before = recorder.effects();
    let result = boundary.with_verified_plan(&current, |plan| recorder.execute(plan));
    assert!(result.is_err(), "stale content reached protocol execution");
    assert_eq!(recorder.effects(), before);
}

#[test]
fn new_uncaptured_source_file_rejects_before_protocol_execution() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let inventory = Fixture::inventory();
    let source = Fixture::source(&receipt, 0);
    let boundary = Boundary::prepare(
        fixture.root(),
        &translated,
        Fixture::inventory(),
        Fixture::source(&receipt, 0),
    )
    .unwrap();
    fs::write(
        fixture.root().join("Assets/NewConsumer.cs"),
        "class NewConsumer {}\n",
    )
    .unwrap();
    let mut recorder = Recorder::default();
    let result = boundary.with_verified_plan(
        &CurrentInputs::new(fixture.root(), &policy, &inventory, &source),
        |plan| recorder.execute(plan),
    );
    assert!(
        result.is_err(),
        "new uncaptured source escaped the complete census"
    );
    assert!(recorder.requests.is_empty());
}

#[test]
fn stale_policy_wrong_pe_and_changed_inventory_reject_before_effects() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let mut policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let source = Fixture::source(&receipt, 0);
    let boundary = Boundary::prepare(
        fixture.root(),
        &translated,
        Fixture::inventory(),
        Fixture::source(&receipt, 0),
    )
    .unwrap();
    let inventory = Fixture::inventory();
    policy.profile_id.push_str("-changed");
    let mut recorder = Recorder::default();
    assert!(
        boundary
            .with_verified_plan(
                &CurrentInputs::new(fixture.root(), &policy, &inventory, &source),
                |plan| recorder.execute(plan)
            )
            .is_err()
    );
    let policy = Fixture::policy(&receipt);
    let wrong_pe = Fixture::source(&receipt, 1);
    assert!(
        boundary
            .with_verified_plan(
                &CurrentInputs::new(fixture.root(), &policy, &inventory, &wrong_pe),
                |plan| recorder.execute(plan)
            )
            .is_err()
    );
    let inventory = TrustedProviderInventory::new();
    assert!(
        boundary
            .with_verified_plan(
                &CurrentInputs::new(fixture.root(), &policy, &inventory, &source),
                |plan| recorder.execute(plan)
            )
            .is_err()
    );
    assert!(recorder.requests.is_empty());
}

#[test]
fn changed_provider_implementation_cannot_reuse_prepared_authority() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let source = Fixture::source(&receipt, 0);
    let boundary = Boundary::prepare(
        fixture.root(),
        &translated,
        Fixture::inventory(),
        Fixture::source(&receipt, 0),
    )
    .unwrap();
    let changed = inventory(ResourceKind::RenderSettings, b"changed runtime binding");
    let mut recorder = Recorder::default();
    assert!(
        boundary
            .with_verified_plan(
                &CurrentInputs::new(fixture.root(), &policy, &changed, &source),
                |plan| recorder.execute(plan)
            )
            .is_err()
    );
    assert!(recorder.requests.is_empty());
}

#[test]
fn wrong_kind_inventory_never_becomes_a_prepared_plan() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let mut wrong = TrustedProviderInventory::new();
    // Even a valid different-kind provider cannot satisfy the selected render capability.
    wrong.register(&InputProvider).unwrap();
    assert!(
        Boundary::prepare(
            fixture.root(),
            &translated,
            wrong,
            Fixture::source(&receipt, 0)
        )
        .is_err()
    );
}

struct InputProvider;
impl worldrunner::compat_content::TrustedProvider for InputProvider {
    fn id(&self) -> &'static str {
        "boundary-unlit"
    }
    fn resource_kind(&self) -> ResourceKind {
        ResourceKind::Input
    }
    fn capabilities(&self) -> Vec<worldrunner::compat_content::CapabilityRequirement> {
        vec![worldrunner::compat_content::CapabilityRequirement {
            id: worldrunner::compat_content::CapabilityId::CanonicalKeyboardSnapshot,
            contract_version: 1,
            resource_kind: ResourceKind::Input,
        }]
    }
    fn implementation_bytes(&self) -> &[u8] {
        b"input only"
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"input boundary evidence"
    }
}
