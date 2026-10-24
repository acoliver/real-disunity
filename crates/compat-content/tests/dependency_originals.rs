use std::{fs, path::PathBuf};
use worldrunner_compat_content::{
    AdmissionOutcome, AdmissionPolicy, DependencyClassification, DependencyPolicy, ImportProfile,
    ImportSidecar, TrustedProviderInventory, import_receipt,
};

#[test]
fn all_fifty_reviewed_locations_match_current_protected_inputs_without_admitting_games() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = base.join("../..");
    for (name, count) in [("snake", 17), ("breakout", 18), ("pong", 15)] {
        let fixture: serde_json::Value = serde_json::from_slice(
            &fs::read(workspace.join(format!("fixtures/{name}.json"))).unwrap(),
        )
        .unwrap();
        let metadata = &fixture["metadata"];
        let profile: ImportProfile =
            serde_json::from_value(metadata["import_profile"].clone()).unwrap();
        let sidecar: ImportSidecar =
            serde_json::from_value(metadata["compatibility_sidecar"].clone()).unwrap();
        let components: AdmissionPolicy =
            serde_json::from_value(metadata["admission_policy"].clone()).unwrap();
        let policy = DependencyPolicy::from_json(
            &fs::read(base.join(format!("profiles/{name}-dependencies.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(policy.rules.len(), count);
        let receipt = import_receipt(
            &workspace.join(format!(".work/games/{name}")),
            metadata["scene"].as_str().unwrap().as_ref(),
            &profile,
            &sidecar,
        )
        .unwrap();
        assert_rules_match(&receipt, &policy);
        let blocked = receipt
            .admit(&components, &policy, &TrustedProviderInventory::new())
            .err()
            .unwrap();
        assert_eq!(blocked.proof.outcome, AdmissionOutcome::Blocked);
        assert_importer_contract(name, &blocked.proof);

        assert_eq!(
            blocked.proof.reference_decisions.len(),
            receipt.dependency_references().len()
        );
        if name == "breakout" {
            assert!(blocked.proof.reference_decisions.iter().any(|d| {
                d.observation
                    .reference
                    .as_ref()
                    .is_some_and(|r| r.guid == "247994e1f5a72c2419c26a37e9334c01")
                    && d.classification == DependencyClassification::Blocked
            }));
            assert!(blocked.proof.reference_decisions.iter().any(|d| {
                d.observation.locator.source_path == "Assets/Settings/UniversalRP.asset"
                    && d.classification == DependencyClassification::Blocked
            }));
        }
        receipt
            .verify_current(&workspace.join(format!(".work/games/{name}")))
            .unwrap();
    }
}

fn assert_rules_match(
    receipt: &worldrunner_compat_content::ContentReceipt,
    policy: &DependencyPolicy,
) {
    for rule in &policy.rules {
        let observed = receipt
            .dependency_references()
            .iter()
            .find(|r| r.locator == rule.locator)
            .unwrap_or_else(|| panic!("unmatched {}", rule.id));
        assert_eq!(observed.source_sha256, rule.source_sha256, "{}", rule.id);
        assert_eq!(
            observed.reference.as_ref(),
            Some(&rule.expected_reference),
            "{}",
            rule.id
        );
        assert_eq!(observed.resource_kind, rule.resource_kind, "{}", rule.id);
    }
}

fn assert_importer_contract(name: &str, proof: &worldrunner_compat_content::AdmissionProof) {
    if name != "pong" {
        assert!(proof.reference_decisions.iter().any(|d| {
            d.observation.resource_kind == worldrunner_compat_content::ResourceKind::ImporterScript
                && d.classification == DependencyClassification::Blocked
                && d.limitations
                    .iter()
                    .any(|s| s.contains("compiled assembly"))
        }));
    }
}
