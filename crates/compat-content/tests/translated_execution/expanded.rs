use super::*;

#[test]
fn new_unmapped_document_without_edges_blocks() {
    use std::io::Write;
    let (root, _, mut policy) = setup();
    std::fs::OpenOptions::new()
        .append(true)
        .open(root.path().join("Assets/Main.unity"))
        .unwrap()
        .write_all(b"--- !u!999 &9\nUnknownCapability:\n  m_Value: 42\n")
        .unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    assert!(
        selection(&receipt, &policy)
            .admit(root.path(), &inventory(), &qualification(&receipt))
            .is_err()
    );
}

#[test]
fn render_capability_cannot_delete_full_quality_settings_or_unknown_records() {
    for path in [
        "ProjectSettings/QualitySettings.asset",
        "ProjectSettings/Unknown.asset",
    ] {
        let (root, _, mut policy) = setup();
        std::fs::write(root.path().join(path), "%YAML 1.1\n--- !u!47 &1\nQualitySettings:\n  m_CurrentQuality: 0\n  m_QualitySettings: []\n").unwrap();
        let receipt = read(&root);
        policy.original_closure_sha256 = receipt.closure_sha256().into();
        policy.operations.push(SelectionOperation::ExcludeRecord {
            id: "delete-settings".into(),
            record: ExecutionRecord::Metadata { path: path.into() },
            source_sha256: receipt.graph().source_hashes[path].clone(),
            expected_sha256: resources::hash(&receipt.graph().metadata_records[path]),
            effect: policy.operations[0].effect().clone(),
        });
        assert!(
            receipt
                .select_execution(
                    &AdmissionPolicy::default(),
                    &DependencyPolicy::default(),
                    &policy
                )
                .is_err(),
            "unrelated capability removed {path}"
        );
    }
}

#[test]
fn render_capability_cannot_redact_an_unknown_settings_field() {
    let (root, _, mut policy) = setup();
    std::fs::write(
        root.path().join(SETTINGS),
        SHADER.replace("m_AlwaysIncludedShaders", "m_UnknownDependency"),
    )
    .unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    if let SelectionOperation::ReplaceField {
        source_sha256,
        field,
        ..
    } = &mut policy.operations[0]
    {
        *source_sha256 = receipt.graph().source_hashes[SETTINGS].clone();
        field[2] = FieldSegment::Key("m_UnknownDependency".into());
    }
    assert!(
        receipt
            .select_execution(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &policy
            )
            .is_err()
    );
}

#[test]
fn effective_incoming_edge_to_excluded_urp_record_blocks() {
    let (root, _, mut policy) = setup();
    let path = "ProjectSettings/URPProjectSettings.asset";
    std::fs::write(root.path().join(path), "%YAML 1.1\n--- !u!114 &1\nMonoBehaviour:\n  m_Script: {fileID: 11500000, guid: 247994e1f5a72c2419c26a37e9334c01, type: 3}\n").unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    policy.operations.push(SelectionOperation::ExcludeRecord {
        id: "opaque".into(),
        record: ExecutionRecord::Metadata { path: path.into() },
        source_sha256: receipt.graph().source_hashes[path].clone(),
        expected_sha256: resources::hash(&receipt.graph().metadata_records[path]),
        effect: SelectionEffect {
            behavior: "opaque-unconsumed".into(),
            lost_effects: vec!["original unknown URP settings".into()],
            reason: "exact unconsumed record".into(),
            capability: None,
        },
    });
    if let SelectionOperation::ReplaceField { replacement, .. } = &mut policy.operations[0] {
        *replacement = json!([{"fileID": 1, "guid":"247994e1f5a72c2419c26a37e9334c01", "type":3}]);
    }
    assert!(
        selection(&receipt, &policy)
            .admit(root.path(), &inventory(), &qualification(&receipt))
            .is_err()
    );
}

#[test]
fn exact_quality_pipeline_selection_preserves_all_tiers_and_other_fields() {
    let (root, _, mut policy) = setup();
    let path = "ProjectSettings/QualitySettings.asset";
    std::fs::write(root.path().join(path), "%YAML 1.1\n--- !u!47 &1\nQualitySettings:\n  m_CurrentQuality: 1\n  m_QualitySettings:\n  - name: Low\n    vSyncCount: 0\n    customRenderPipeline: {fileID: 0}\n  - name: High\n    vSyncCount: 2\n    customRenderPipeline: {fileID: 11400000, guid: 22222222222222222222222222222222, type: 2}\n").unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    let original = receipt.graph().metadata_records[path].clone();
    policy.operations.push(SelectionOperation::ReplaceField {
        id: "high-tier-pipeline".into(),
        record: ExecutionRecord::Metadata { path: path.into() },
        source_sha256: receipt.graph().source_hashes[path].clone(),
        field: vec![
            FieldSegment::Key("documents".into()),
            FieldSegment::Index(0),
            FieldSegment::Key("m_QualitySettings".into()),
            FieldSegment::Index(1),
            FieldSegment::Key("customRenderPipeline".into()),
        ],
        expected: original["documents"][0]["m_QualitySettings"][1]["customRenderPipeline"].clone(),
        replacement: json!({"fileID":0}),
        effect: policy.operations[0].effect().clone(),
    });
    let translated = selection(&receipt, &policy);
    let admitted = translated
        .admit(root.path(), &inventory(), &qualification(&receipt))
        .unwrap();
    let mut expected = original.clone();
    expected["documents"][0]["m_QualitySettings"][1]["customRenderPipeline"] = json!({"fileID":0});
    assert_eq!(admitted.execution_graph().metadata_records[path], expected);
    assert_eq!(admitted.original().graph().metadata_records[path], original);
}
