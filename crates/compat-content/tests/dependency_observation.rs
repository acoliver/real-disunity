use std::fs;
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_receipt};

#[test]
fn receipt_observes_original_settings_identity_and_reference_type() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Assets")).unwrap();
    fs::create_dir(root.path().join("ProjectSettings")).unwrap();
    fs::write(
        root.path().join("Assets/Main.unity"),
        "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n",
    )
    .unwrap();
    fs::write(
        root.path().join("Assets/Main.unity.meta"),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    let original = "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_AlwaysIncludedShaders:\n  - {fileID: 7, guid: 0000000000000000f000000000000000, type: 0}\n";
    fs::write(
        root.path().join("ProjectSettings/GraphicsSettings.asset"),
        original,
    )
    .unwrap();
    let receipt = import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let proof = receipt.proof();
    let refs = proof["dependency_references"]
        .as_array()
        .expect("receipt must retain located original references separately from resolved graph");
    let shader = refs
        .iter()
        .find(|r| r["locator"]["source_path"] == "ProjectSettings/GraphicsSettings.asset")
        .unwrap();
    assert_eq!(shader["locator"]["owner"]["class_id"], 30);
    assert_eq!(shader["locator"]["owner"]["file_id"], 1);
    assert_eq!(shader["reference"]["reference_type"], 0);
    assert_eq!(shader["resource_kind"], "shader");
    assert_eq!(
        receipt.sources()["ProjectSettings/GraphicsSettings.asset"],
        original.as_bytes()
    );
    assert!(
        receipt.graph().metadata_records["ProjectSettings/GraphicsSettings.asset"]["documents"][0]
            ["m_AlwaysIncludedShaders"][0]
            .get("$missing")
            .is_some()
    );
}
