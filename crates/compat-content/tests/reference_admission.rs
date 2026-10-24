use std::{cell::Cell, fs};
use worldrunner_compat_content::{
    AdmissionPolicy, ImportProfile, ImportSidecar, import_project, materialize,
};

#[test]
fn dangling_or_malformed_transform_parent_rejects_before_native_requests() {
    for parent in ["{fileID: 99}", "{fileID: 1, guid: 123}"] {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("Assets")).unwrap();
        fs::write(temp.path().join("Assets/Main.unity"), format!("%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  m_Component:\n  - component: {{fileID: 2}}\n--- !u!4 &2\nTransform:\n  m_GameObject: {{fileID: 1}}\n  m_Father: {parent}\n")).unwrap();
        fs::write(
            temp.path().join("Assets/Main.unity.meta"),
            "guid: 11111111111111111111111111111111\n",
        )
        .unwrap();
        let graph = import_project(
            temp.path(),
            "Assets/Main.unity".as_ref(),
            &ImportProfile::default(),
            &ImportSidecar::default(),
        )
        .unwrap();
        let calls = Cell::new(0);
        let result = materialize(&graph, &AdmissionPolicy::default(), |_| {
            calls.set(calls.get() + 1);
            Err("unexpected native request")
        });
        assert_eq!(
            calls.get(),
            0,
            "unresolved source parent reached native execution"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unresolved reference")
        );
    }
}
