mod dependency_support;
use dependency_support::{EDITOR, SETTINGS, project, read};
use std::fs;
use worldrunner_compat_content::{AdmissionPolicy, DependencyPolicy, TrustedProviderInventory};

#[test]
fn resolved_material_asset_is_not_a_shader_provider() {
    let root = project(EDITOR);
    let original = read(&root);
    assert!(
        original
            .admit(
                &AdmissionPolicy::default(),
                &dependency_support::editor_policy(&original),
                &TrustedProviderInventory::new()
            )
            .is_ok()
    );
    fs::remove_file(root.path().join(SETTINGS)).unwrap();
    fs::write(root.path().join("ProjectSettings/GraphicsSettings.asset"), "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_AlwaysIncludedShaders:\n  - {fileID: 2100000, guid: 22222222222222222222222222222222, type: 2}\n").unwrap();
    fs::write(
        root.path().join("Assets/Fake.mat"),
        "%YAML 1.1\n--- !u!21 &2100000\nMaterial:\n  m_Name: FakeShader\n",
    )
    .unwrap();
    fs::write(
        root.path().join("Assets/Fake.mat.meta"),
        "guid: 22222222222222222222222222222222\n",
    )
    .unwrap();
    let receipt = read(&root);
    assert!(
        receipt
            .admit(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new()
            )
            .is_err()
    );
}
