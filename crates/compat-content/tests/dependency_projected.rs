mod dependency_support;
use dependency_support::{EDITOR, editor_policy, project, read};
use serde_json::json;
use std::fs;
use worldrunner_compat_content::{
    AdmissionPolicy, FieldOverride, ImportProfile, ImportSidecar, OverlayPrecondition,
    TrustedProviderInventory, import_receipt,
};

#[test]
fn graph_only_builtin_injected_by_overlay_is_never_silently_unclassified() {
    let root = project(EDITOR);
    let original = read(&root);
    let policy = editor_policy(&original);
    let mut sidecar = ImportSidecar::default();
    sidecar.field_overrides.push(FieldOverride {
        guid: "11111111111111111111111111111111".into(),
        file_id: 1,
        property_path: "m_Name".into(),
        precondition: Some(OverlayPrecondition {
            schema_version: 1,
            class_id: 1,
            expected_value: json!("Original"),
            source_sha256: None,
        }),
        value: json!({"$builtin":"invented"}),
        reason: "negative admission control".into(),
    });
    let receipt = import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap();
    assert!(
        receipt
            .admit(
                &AdmissionPolicy::default(),
                &policy,
                &TrustedProviderInventory::new()
            )
            .is_err()
    );
}
#[test]
fn source_reserved_reference_syntax_does_not_claim_provider_readiness() {
    let root = project(EDITOR);
    let scene = fs::read_to_string(root.path().join("Assets/Main.unity"))
        .unwrap()
        .replace("m_Name: Original", "m_Name: {$builtin: invented}");
    fs::write(root.path().join("Assets/Main.unity"), scene).unwrap();
    let receipt = read(&root);
    assert!(
        receipt
            .admit(
                &AdmissionPolicy::default(),
                &editor_policy(&receipt),
                &TrustedProviderInventory::new()
            )
            .is_err()
    );
}
