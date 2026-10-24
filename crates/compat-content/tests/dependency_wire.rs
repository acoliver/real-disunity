mod dependency_support;
use dependency_support::{EDITOR, editor_policy, project, read};
use serde_json::{Value, json};
use worldrunner_compat_content::{
    AdmissionPolicy, AdmissionProof, DependencyPolicy, TrustedProviderInventory,
};

#[test]
fn dependency_policy_rejects_ambiguous_and_invalid_wire_inputs() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let original = serde_json::to_value(editor_policy(&receipt)).unwrap();
    let controls = [
        ("/schema_version", json!(2)),
        ("/unknown", json!(true)),
        ("/fidelity", json!("anything")),
        ("/rules/0/locator/source_path", json!("../outside")),
        ("/rules/0/source_sha256", json!("bad")),
        ("/rules/0/expected_reference/guid", json!("bad")),
        (
            "/rules/0/expected_reference/file_id",
            json!(18_446_744_073_709_551_615_u64),
        ),
        (
            "/rules/0/expected_reference/reference_type",
            json!(2_147_483_648_i64),
        ),
        ("/rules/0/reason", json!("")),
        ("/rules/0/evidence", json!([])),
        ("/rules/0/locator/field/0/segment", json!("wildcard")),
        ("/rules/0/locator/owner/extra", json!(1)),
        (
            "/rules/0/disposition/classification",
            json!("ignore_everything"),
        ),
    ];
    for (pointer, value) in controls {
        let mut input = original.clone();
        if let Some(target) = input.pointer_mut(pointer) {
            *target = value;
        } else if pointer == "/unknown" {
            input["unknown"] = value;
        } else {
            input["rules"][0]["locator"]["owner"]["extra"] = value;
        }
        assert!(
            DependencyPolicy::from_json(&serde_json::to_vec(&input).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut duplicate = original.clone();
    let rule = duplicate["rules"][0].clone();
    duplicate["rules"].as_array_mut().unwrap().push(rule);
    assert!(DependencyPolicy::from_json(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let text = serde_json::to_string(&original).unwrap();
    for text in [
        text.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        ),
        text.replacen(
            "\"file_id\":13964",
            "\"file_id\":13964,\"file_id\":13964",
            1,
        ),
    ] {
        assert!(DependencyPolicy::from_json(text.as_bytes()).is_err());
    }
}
#[test]
fn proof_round_trip_is_sealed_and_unknown_decisions_reject() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &editor_policy(&receipt),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let bytes = serde_json::to_vec(admitted.proof()).unwrap();
    assert_eq!(
        AdmissionProof::from_json(&bytes).unwrap().admission_sha256,
        admitted.proof().admission_sha256
    );
    let mut forged: Value = serde_json::from_slice(&bytes).unwrap();
    forged["reference_decisions"][0]["classification"] = json!("ignored");
    assert!(AdmissionProof::from_json(&serde_json::to_vec(&forged).unwrap()).is_err());
}
