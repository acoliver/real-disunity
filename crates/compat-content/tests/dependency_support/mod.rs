use std::fs;
use worldrunner_compat_content::*;
pub const SETTINGS: &str = "ProjectSettings/PackageManagerSettings.asset";
pub const EDITOR: &str = "%YAML 1.1\n--- !u!114 &1\nMonoBehaviour:\n  m_Script: {fileID: 13964, guid: 0000000000000000e000000000000000, type: 0}\n  m_EditorClassIdentifier: UnityEditor.dll::UnityEditor.PackageManager.UI.Internal.PackageManagerProjectSettings\n";
pub fn project(settings: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Assets")).unwrap();
    fs::create_dir(root.path().join("ProjectSettings")).unwrap();
    fs::write(root.path().join("Assets/Main.unity"), "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  m_Component:\n  - component: {fileID: 2}\n--- !u!4 &2\nTransform:\n  m_GameObject: {fileID: 1}\n  m_Father: {fileID: 0}\n").unwrap();
    fs::write(
        root.path().join("Assets/Main.unity.meta"),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    fs::write(root.path().join(SETTINGS), settings).unwrap();
    root
}
pub fn read(root: &tempfile::TempDir) -> ContentReceipt {
    import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap()
}
pub fn editor_policy(receipt: &ContentReceipt) -> DependencyPolicy {
    let reference = receipt
        .dependency_references()
        .iter()
        .find(|r| r.locator.source_path == SETTINGS)
        .unwrap();
    DependencyPolicy {
        schema_version: 1,
        profile_id: "reviewed-test-unlit".into(),
        fidelity: Fidelity::TranslatedUnlitKeyboardSubset,
        required_capabilities: vec![],
        rules: vec![DependencyRule {
            id: "editor".into(),
            locator: reference.locator.clone(),
            source_sha256: reference.source_sha256.clone(),
            expected_reference: reference.reference.clone().unwrap(),
            resource_kind: ResourceKind::Script,
            disposition: DependencyDisposition::EditorConfiguration,
            evidence: vec![EvidenceRef {
                path: SETTINGS.into(),
                sha256: reference.source_sha256.clone(),
                line: 4,
                confidence: Confidence::Direct,
            }],
            reason: "Explicit editor configuration record, unused by selected local execution"
                .into(),
        }],
    }
}
