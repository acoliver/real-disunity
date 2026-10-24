use std::fs;
use worldrunner_compat_content::*;

const GUID: &str = "22222222222222222222222222222222";
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Assets")).unwrap();
    fs::write(root.path().join("Assets/Main.unity"), format!("%YAML 1.1\n--- !u!1001 &100\nPrefabInstance:\n  m_SourcePrefab: {{fileID: 100100000, guid: {GUID}, type: 3}}\n  m_Modification:\n    m_TransformParent: {{fileID: 0}}\n    m_Modifications: []\n--- !u!4 &200 stripped\nTransform:\n  m_CorrespondingSourceObject: {{fileID: 11, guid: {GUID}, type: 3}}\n  m_PrefabInstance: {{fileID: 100}}\n")).unwrap();
    fs::write(
        root.path().join("Assets/Main.unity.meta"),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    fs::write(root.path().join("Assets/Template.prefab"), "%YAML 1.1\n--- !u!1001 &100100000\nPrefab:\n  m_RootGameObject: {fileID: 10}\n--- !u!1 &10\nGameObject:\n  m_Name: Template\n  m_Component:\n  - component: {fileID: 11}\n--- !u!4 &11\nTransform:\n  m_GameObject: {fileID: 10}\n  m_Father: {fileID: 0}\n").unwrap();
    fs::write(
        root.path().join("Assets/Template.prefab.meta"),
        format!("guid: {GUID}\n"),
    )
    .unwrap();
    root
}
#[test]
fn stripped_prefab_provenance_retains_original_available_edges() {
    let root = fixture();
    let receipt = import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let edges: Vec<_> = admitted
        .proof()
        .reference_decisions
        .iter()
        .filter(|d| {
            matches!(
                d.observation.locator.owner,
                OwnerIdentity::AssetDocument { file_id: 200, .. }
            )
        })
        .collect();
    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|d| d.classification == DependencyClassification::RequiredRuntime)
    );
    admitted.verify_current(root.path()).unwrap();
}
