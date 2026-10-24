use serde_json::json;
use std::{fs, path::Path};
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};

const SCENE: &str = "Assets/Main.unity";
const GUID: &str = "11111111111111111111111111111111";

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("Assets")).unwrap();
    fs::write(temp.path().join(SCENE), "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  m_IsActive: 1\n  m_Component: []\n").unwrap();
    fs::write(
        temp.path().join(format!("{SCENE}.meta")),
        format!("fileFormatVersion: 2\nguid: {GUID}\n"),
    )
    .unwrap();
    temp
}

fn import(
    root: &Path,
    sidecar: &ImportSidecar,
) -> Result<worldrunner_compat_content::ImportedScene, worldrunner_compat_content::ImportError> {
    import_project(root, SCENE.as_ref(), &ImportProfile::default(), sidecar)
}

#[test]
fn metadata_references_have_located_missing_and_malformed_diagnostics() {
    let temp = fixture();
    let meta = temp.path().join(format!("{SCENE}.meta"));
    let text = fs::read_to_string(&meta).unwrap();
    fs::write(meta, format!("{text}Importer:\n  external: {{fileID: 7, guid: 22222222222222222222222222222222}}\n  invalid: {{fileID: 1.5}}\n")).unwrap();
    let graph = import(temp.path(), &ImportSidecar::default()).unwrap();
    for (code, field) in [
        ("missing_guid", "Importer.external"),
        ("invalid_reference_id", "Importer.invalid"),
    ] {
        assert!(
            graph.diagnostics.iter().any(|d| d.code == code
                && d.path == format!("{SCENE}.meta")
                && d.field.as_deref() == Some(field)),
            "{code}: {:?}",
            graph.diagnostics
        );
    }
}

#[test]
fn overlay_rejects_changed_old_value_and_class_instead_of_overwriting_source_drift() {
    let temp = fixture();
    let sidecar: ImportSidecar = serde_json::from_value(json!({"field_overrides":[{
        "guid":GUID,"file_id":1,"property_path":"m_Name","value":"Replacement","reason":"Authored test",
        "precondition":{"schema_version":1,"class_id":1,"expected_value":"Original"}
    }]})).unwrap();
    assert_eq!(
        import(temp.path(), &sidecar).unwrap().nodes[0].name,
        "Replacement"
    );
    for text in [
        "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Updated\n",
        "%YAML 1.1\n--- !u!114 &1\nMonoBehaviour:\n  m_Name: Original\n",
    ] {
        fs::write(temp.path().join(SCENE), text).unwrap();
        let result = import(temp.path(), &sidecar);
        assert!(result.is_err(), "stale overlay silently applied");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("overlay precondition")
        );
    }
}

#[test]
fn overlay_without_explicit_precondition_is_rejected() {
    let temp = fixture();
    let sidecar = serde_json::from_value(json!({"field_overrides":[{
        "guid":GUID,"file_id":1,"property_path":"m_Name","value":"Replacement","reason":"Authored test"
    }]})).unwrap();
    assert!(import(temp.path(), &sidecar).is_err());
}

#[cfg(unix)]
#[test]
fn selected_source_symlink_is_rejected_even_inside_root() {
    let temp = fixture();
    fs::rename(
        temp.path().join(SCENE),
        temp.path().join("Assets/Actual.unity"),
    )
    .unwrap();
    std::os::unix::fs::symlink("Actual.unity", temp.path().join(SCENE)).unwrap();
    let result = import(temp.path(), &ImportSidecar::default());
    assert!(result.is_err(), "selected symlink was followed");
    assert!(result.unwrap_err().to_string().contains("symlink"));
}

fn environment_fixture(root: &Path) {
    fs::create_dir_all(root.join("Assets/Scripts")).unwrap();
    fs::create_dir_all(root.join("ProjectSettings")).unwrap();
    fs::write(root.join("Assets/Scripts/Helper.cs"), "class Helper {}\n").unwrap();
    fs::write(root.join("ProjectSettings/Settings.asset"), "%YAML 1.1\n--- !u!129 &1\nSettings:\n  target: {fileID: 7, guid: 22222222222222222222222222222222}\n").unwrap();
    fs::write(
        root.join("Assets/Other.asset"),
        "%YAML 1.1\n--- !u!114 &7\nMonoBehaviour:\n  m_Name: Dependency\n",
    )
    .unwrap();
    fs::write(
        root.join("Assets/Other.asset.meta"),
        "guid: 22222222222222222222222222222222\n",
    )
    .unwrap();
}

#[test]
fn receipt_owns_transitive_metadata_payloads_and_unserialized_runtime_and_settings() {
    let temp = fixture();
    environment_fixture(temp.path());
    let receipt = worldrunner_compat_content::import_receipt(
        temp.path(),
        SCENE.as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    for path in [
        SCENE,
        "Assets/Main.unity.meta",
        "Assets/Other.asset",
        "Assets/Other.asset.meta",
        "Assets/Scripts/Helper.cs",
        "ProjectSettings/Settings.asset",
    ] {
        assert_eq!(
            receipt.sources().get(path),
            Some(&fs::read(temp.path().join(path)).unwrap()),
            "missing owned input {path}"
        );
    }
    let old = receipt.sources()[SCENE].clone();
    fs::write(
        temp.path().join(SCENE),
        "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Future\n",
    )
    .unwrap();
    assert_eq!(
        receipt.sources()[SCENE],
        old,
        "receipt exposed mutable bytes"
    );
    assert!(
        receipt.verify_current(temp.path()).is_err(),
        "stale receipt adopted"
    );
    let fresh = worldrunner_compat_content::import_receipt(
        temp.path(),
        SCENE.as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    assert_eq!(fresh.graph().nodes[0].name, "Future");
}

#[test]
fn receipt_rejects_additive_script_drift_but_fresh_import_accepts_it() {
    let temp = fixture();
    let receipt = worldrunner_compat_content::import_receipt(
        temp.path(),
        SCENE.as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("Assets/Scripts")).unwrap();
    fs::write(temp.path().join("Assets/Scripts/New.cs"), "class New {}\n").unwrap();
    assert!(
        receipt.verify_current(temp.path()).is_err(),
        "additive source drift missed"
    );
    let fresh = worldrunner_compat_content::import_receipt(
        temp.path(),
        SCENE.as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    assert!(fresh.sources().contains_key("Assets/Scripts/New.cs"));
}

#[test]
fn malformed_external_guid_never_falls_back_to_local_object_identity() {
    let temp = fixture();
    fs::write(temp.path().join(SCENE), "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  m_Component: []\n  external: {fileID: 1, guid: 123}\n").unwrap();
    let graph = import(temp.path(), &ImportSidecar::default()).unwrap();
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid_reference_guid" && d.field.as_deref() == Some("external")),
        "malformed GUID silently became local: {:?}",
        graph.documents
    );
}

#[test]
fn metadata_unresolved_reference_rejects_admission_before_any_native_request() {
    let temp = fixture();
    let path = temp.path().join(SCENE);
    let text = fs::read_to_string(&path).unwrap().replace(
        "m_Component: []",
        "m_Component:\n  - component: {fileID: 2}",
    );
    fs::write(path, format!("{text}--- !u!4 &2\nTransform:\n  m_GameObject: {{fileID: 1}}\n  m_Father: {{fileID: 0}}\n")).unwrap();
    let meta = temp.path().join(format!("{SCENE}.meta"));
    let text = fs::read_to_string(&meta).unwrap();
    fs::write(
        meta,
        format!(
            "{text}Importer:\n  external: {{fileID: 7, guid: 22222222222222222222222222222222}}\n"
        ),
    )
    .unwrap();
    let graph = import(temp.path(), &ImportSidecar::default()).unwrap();
    let result = worldrunner_compat_content::materialization_plan(
        &graph,
        &worldrunner_compat_content::AdmissionPolicy::default(),
    );
    assert!(
        result.is_err(),
        "metadata dependency was admitted unresolved"
    );
    assert!(result.unwrap_err().to_string().contains("Main.unity.meta"));
}
