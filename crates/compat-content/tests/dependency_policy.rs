mod dependency_support;
use dependency_support::*;
use worldrunner_compat_content::*;

#[test]
fn exact_editor_rule_retains_edge_without_invented_provider() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let policy = editor_policy(&receipt);
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &policy,
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let proof = serde_json::to_value(admitted.proof()).unwrap();
    assert_eq!(proof["outcome"], "admitted_translated");
    let decision = proof["reference_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["rule_id"] == "editor")
        .unwrap();
    assert_eq!(
        decision["observation"]["resolution"]["status"],
        "unresolved"
    );
    assert!(decision["provider_binding"].is_null());
    assert!(
        receipt.graph().metadata_records[SETTINGS]["documents"][0]["m_Script"]
            .get("$missing")
            .is_some()
    );
    assert!(!admitted.materialization_plan().commands.is_empty());
}

#[test]
fn strict_reports_located_unknown_and_never_issues_native_requests() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let err = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .err()
        .unwrap();
    assert_eq!(err.proof.outcome, AdmissionOutcome::Blocked);
    assert!(
        err.proof
            .reference_decisions
            .iter()
            .any(|d| d.classification == DependencyClassification::Blocked)
    );
    let calls = std::cell::Cell::new(0);
    assert!(
        materialize(receipt.graph(), &AdmissionPolicy::default(), |_| {
            calls.set(calls.get() + 1);
            Err("unexpected")
        })
        .is_err()
    );
    assert_eq!(calls.get(), 0);
}
