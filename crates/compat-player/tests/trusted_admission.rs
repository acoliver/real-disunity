//! Synthetic protocol-boundary controls. These tests do not boot native/CLR/GPU
//! services and do not qualify a provider, PE inspector, or original game.
mod providers {
    pub use compat_player::providers::{GameSource, QualifiedProviders};
}
#[path = "../src/image_lease.rs"]
pub mod image_lease;
#[path = "../src/trusted.rs"]
pub mod trusted;
use trusted::protocol;
#[path = "trusted_admission/controls.rs"]
mod controls;
#[path = "trusted_admission/fixture.rs"]
mod fixture;

use fixture::{Fixture, Recorder};
use protocol::{Boundary, CurrentInputs};
use worldrunner::compat_content::{AdmissionOutcome, TrustedProviderInventory};

#[test]
fn actual_opaque_admitted_plan_emits_original_scene_without_admitting_original_pipeline() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let inventory = Fixture::inventory();
    let source = Fixture::source(&receipt, 0);
    let boundary = Boundary::prepare(fixture.root(), &translated, inventory, source).unwrap();
    assert_eq!(
        boundary.proof().original_proof.outcome,
        AdmissionOutcome::Blocked
    );
    let current_inventory = Fixture::inventory();
    let current_source = Fixture::source(&receipt, 0);
    let current = CurrentInputs::new(fixture.root(), &policy, &current_inventory, &current_source);
    let mut recorder = Recorder::default();
    let materialized = boundary
        .with_verified_plan(&current, |plan| recorder.execute(plan))
        .unwrap();
    assert_eq!(materialized.object_ids.len(), 1);
    assert_eq!(recorder.objects(), vec!["Original"]);
    assert!(recorder.requests.iter().all(|r| r["op"] != "frame.begin"));
    assert!(recorder.requests.iter().all(|r| r["op"] != "input.set"));
}

#[test]
fn failed_prepare_leaves_queued_effects_and_native_request_count_unchanged() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let mut recorder = Recorder::default();
    recorder.queue("old-session-input");
    let before = recorder.effects();
    let failed = Boundary::prepare(
        fixture.root(),
        &translated,
        TrustedProviderInventory::new(),
        Fixture::source(&receipt, 0),
    );
    assert!(failed.is_err());
    assert_eq!(recorder.effects(), before);
}

#[test]
fn unchanged_preparation_still_refuses_activation_without_loader_contracts() {
    use trusted::{ActivationPrerequisite, LoadError};
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let boundary = Boundary::prepare(
        fixture.root(),
        &translated,
        Fixture::inventory(),
        Fixture::source(&receipt, 0),
    )
    .unwrap();
    let error = trusted::load_boundary(&boundary, &policy).unwrap_err();
    let LoadError::ActivationUnavailable(required) = error else {
        panic!("current inputs must reach the explicit activation blocker");
    };
    assert_eq!(
        required,
        [
            ActivationPrerequisite::ReleasedQualifiedProviderTuple,
            ActivationPrerequisite::ImmutableProjectProviderAndGameImageLeases,
            ActivationPrerequisite::CompleteTransitiveGameApiSignatureProof,
            ActivationPrerequisite::LoadedGameImageIdentityAndLifecycleCleanup,
        ]
    );
}

#[test]
fn load_reports_changed_policy_before_activation_prerequisites() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let mut policy = Fixture::policy(&receipt);
    let translated = Fixture::select(&receipt, &policy);
    let boundary = Boundary::prepare(
        fixture.root(),
        &translated,
        Fixture::inventory(),
        Fixture::source(&receipt, 0),
    )
    .unwrap();
    policy.profile_id.push_str("-changed-after-prepare");
    assert!(matches!(
        trusted::load_boundary(&boundary, &policy),
        Err(trusted::LoadError::InputsChanged(_)),
    ));
}
