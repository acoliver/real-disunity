mod dependency_support;
use dependency_support::{project, read};
use serde_json::{Value, json};
use std::fs;
use worldrunner_compat_content::*;

const GUID: &str = "22222222222222222222222222222222";
const POINTER: &str = "{fileID: 11400000, guid: 22222222222222222222222222222222, type: 2}";

fn fixture(container: &str) -> tempfile::TempDir {
    let root = project("");
    fs::write(
        root.path().join("Assets/Data.asset"),
        "%YAML 1.1\n--- !u!114 &11400000\nMonoBehaviour:\n  m_Name: Data\n",
    )
    .unwrap();
    fs::write(
        root.path().join("Assets/Data.asset.meta"),
        format!("guid: {GUID}\n"),
    )
    .unwrap();
    if container == "script_meta" {
        fs::create_dir(root.path().join("Assets/Scripts")).unwrap();
        fs::write(
            root.path().join("Assets/Scripts/Helper.cs"),
            "class Helper {}\n",
        )
        .unwrap();
    }
    let (path, text) = match container {
        "settings" => (
            "ProjectSettings/Test.asset",
            format!("%YAML 1.1\n--- !u!30 &7\nGraphicsSettings:\n  m_Unused: {POINTER}\n"),
        ),
        "importer" => (
            "Assets/Main.unity.meta",
            format!(
                "guid: 11111111111111111111111111111111\nDefaultImporter:\n  userData: {POINTER}\n"
            ),
        ),
        "settings_meta" => (
            "ProjectSettings/Additional.meta",
            format!(
                "guid: 44444444444444444444444444444444\nDefaultImporter:\n  userData: {POINTER}\n"
            ),
        ),
        "script_meta" => (
            "Assets/Scripts/Helper.cs.meta",
            format!(
                "guid: 44444444444444444444444444444444\nDefaultImporter:\n  userData: {POINTER}\n"
            ),
        ),
        _ => (
            "Assets/Unused.asset",
            format!("%YAML 1.1\n--- !u!114 &9\nMonoBehaviour:\n  m_Unused: {POINTER}\n"),
        ),
    };
    fs::write(root.path().join(path), text).unwrap();
    if container == "asset" {
        fs::write(
            root.path().join("Assets/Unused.asset.meta"),
            "guid: 33333333333333333333333333333333\n",
        )
        .unwrap();
        let scene = fs::read_to_string(root.path().join("Assets/Main.unity")).unwrap();
        fs::write(root.path().join("Assets/Main.unity"), scene.replace("m_Name: Original", "m_Name: Original\n  m_Unused: {fileID: 9, guid: 33333333333333333333333333333333, type: 2}")).unwrap();
    }
    root
}
fn projected(root: &tempfile::TempDir, replacement: Value) -> ContentReceipt {
    let sidecar = ImportSidecar {
        reference_substitutions: vec![ReferenceSubstitution {
            guid: GUID.into(),
            file_id: Some(11_400_000),
            replacement,
            reason: "public admission regression".into(),
        }],
        ..ImportSidecar::default()
    };
    import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap()
}
fn blocked(receipt: &ContentReceipt) -> AdmissionProof {
    *receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .err()
        .expect("unclassified projection must block")
        .proof
}
fn marker_requires_classification(container: &str) {
    let root = fixture(container);
    assert!(
        read(&root)
            .admit(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new()
            )
            .is_ok()
    );
    let receipt = projected(
        &root,
        json!({"payload":{"$builtin":"unregistered-provider"}}),
    );
    let proof = blocked(&receipt);
    let original = proof
        .reference_decisions
        .iter()
        .find(|d| {
            d.observation
                .reference
                .as_ref()
                .is_some_and(|r| r.guid == GUID)
        })
        .unwrap();
    assert_eq!(original.classification, DependencyClassification::Blocked);
    let introduced: Vec<_> = proof
        .reference_decisions
        .iter()
        .filter(|d| d.observation.raw_reference == json!({"$builtin":"unregistered-provider"}))
        .collect();
    assert_eq!(introduced.len(), 1);
    assert_eq!(
        introduced[0].classification,
        DependencyClassification::Blocked
    );
    assert_eq!(
        introduced[0].observation.locator.owner,
        original.observation.locator.owner
    );
    assert_eq!(
        introduced[0].observation.locator.field.last(),
        Some(&FieldSegment::Key("payload".into()))
    );
}
#[test]
fn settings_projected_builtin_requires_classification() {
    marker_requires_classification("settings");
}
#[test]
fn importer_metadata_projected_builtin_requires_classification() {
    marker_requires_classification("importer");
}
#[test]
fn all_nested_marker_occurrences_retain_exact_owner_and_array_locations() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        let root = fixture(container);
        let receipt = projected(
            &root,
            json!({"payload":[{"$builtin":"same"},{"$builtin":"same"},{"name":{"$ref":"fake"}},{"$type":"fake"},{"$missing":"fake"}]}),
        );
        let proof = blocked(&receipt);
        let markers: Vec<_> = proof
            .reference_decisions
            .iter()
            .filter(|d| d.observation.reference.is_none())
            .collect();
        assert_eq!(markers.len(), 5, "{container}");
        let original = proof
            .reference_decisions
            .iter()
            .find(|d| {
                d.observation
                    .reference
                    .as_ref()
                    .is_some_and(|r| r.guid == GUID)
            })
            .unwrap();
        for (index, marker) in markers.iter().enumerate() {
            assert_eq!(marker.classification, DependencyClassification::Blocked);
            assert_eq!(
                marker.observation.locator.owner,
                original.observation.locator.owner
            );
            assert!(
                marker
                    .observation
                    .locator
                    .field
                    .contains(&FieldSegment::Index(u32::try_from(index).unwrap()))
            );
        }
    }
}
#[test]
fn changed_reference_shapes_cannot_inherit_available_original_resolution() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        for replacement in [
            json!("replaced"),
            json!(null),
            json!([7]),
            json!({"payload":17}),
        ] {
            let receipt = projected(&fixture(container), replacement);
            let proof = blocked(&receipt);
            assert!(proof.reference_decisions.iter().any(|d| {
                d.observation
                    .reference
                    .as_ref()
                    .is_some_and(|r| r.guid == GUID)
                    && d.classification == DependencyClassification::Blocked
            }));
        }
    }
}
#[test]
fn reference_markers_do_not_shadow_nested_fake_asset_or_name_markers() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        let replacement = json!({"$ref":format!("{GUID}:11400000"), "guid":GUID, "file_id":11_400_000,
            "asset":{"$builtin":"fake", "name":{"$ref":"fake"}}});
        let receipt = projected(&fixture(container), replacement);
        let proof = blocked(&receipt);
        assert_eq!(
            proof
                .reference_decisions
                .iter()
                .filter(|d| d.observation.reference.is_none())
                .count(),
            2
        );
    }
}
#[test]
fn resolved_marker_target_must_match_its_original_guid_and_file_id() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        let replacement =
            json!({"$ref":"11111111111111111111111111111111:1", "guid":GUID, "file_id":11_400_000});
        blocked(&projected(&fixture(container), replacement));
    }
}
#[test]
fn source_reserved_markers_are_observed_in_settings_importers_and_objectless_records() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        let root = fixture(container);
        let path = match container {
            "settings" => "ProjectSettings/Test.asset",
            "importer" => "Assets/Main.unity.meta",
            "settings_meta" => "ProjectSettings/Additional.meta",
            "script_meta" => "Assets/Scripts/Helper.cs.meta",
            _ => "Assets/Unused.asset",
        };
        let text = fs::read_to_string(root.path().join(path)).unwrap().replace(
            POINTER,
            "{fileID: 11400000, name: {$ref: fake}, asset: {$builtin: fake}}",
        );
        fs::write(root.path().join(path), text).unwrap();
        let proof = blocked(&read(&root));
        let reserved: Vec<_> = proof.reference_decisions.iter().filter(|d| matches!(&d.observation.resolution, ReferenceResolution::Unresolved {code} if code == "reserved_reference_syntax_in_original_source")).collect();
        assert_eq!(reserved.len(), 2, "{container}");
    }
}

#[test]
fn familiar_names_and_malformed_or_unavailable_reference_markers_cannot_authorize_metadata() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        for replacement in [
            json!({"payload":{"$builtin":"canonical_unlit_quads"}}),
            json!({"$ref":format!("{GUID}:999"),"guid":GUID,"file_id":11_400_000}),
            json!({"$ref":false,"guid":GUID,"file_id":11_400_000}),
            json!({"$builtin":null}),
        ] {
            blocked(&projected(&fixture(container), replacement));
        }
    }
}
#[test]
fn multiple_settings_documents_keep_distinct_original_owner_and_projected_ledger() {
    let root = fixture("settings");
    let path = root.path().join("ProjectSettings/Test.asset");
    let first = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        format!("{first}--- !u!30 &8\nGraphicsSettings:\n  m_Unused: {POINTER}\n"),
    )
    .unwrap();
    let receipt = projected(
        &root,
        json!({"payload":{"$builtin":"unregistered-provider"}}),
    );
    let proof = blocked(&receipt);
    let markers: Vec<_> = proof
        .reference_decisions
        .iter()
        .filter(|d| d.observation.reference.is_none())
        .collect();
    assert_eq!(markers.len(), 2);
    for (index, marker) in markers.iter().enumerate() {
        assert!(
            matches!(marker.observation.locator.owner, OwnerIdentity::SettingsDocument {document_index, file_id, class_id:30, ..} if document_index == u32::try_from(index).unwrap() && file_id == 7 + i64::try_from(index).unwrap())
        );
    }
    let bytes = serde_json::to_vec(&proof).unwrap();
    let parsed = AdmissionProof::from_json(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        serde_json::to_value(&proof).unwrap()
    );
    let mut forged = serde_json::to_value(&proof).unwrap();
    forged["reference_decisions"] = json!([]);
    assert!(AdmissionProof::from_json(&serde_json::to_vec(&forged).unwrap()).is_err());
}
#[test]
fn exact_editor_classification_and_fresh_compatible_inputs_remain_valid() {
    let root = project(dependency_support::EDITOR);
    let original = read(&root);
    let policy = dependency_support::editor_policy(&original);
    let components = AdmissionPolicy::default();
    let providers = TrustedProviderInventory::new();
    let admitted = original
        .admit(&components, &policy, &providers)
        .unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir(root.path().join("Assets/Scripts")).unwrap();
    fs::write(
        root.path().join("Assets/Scripts/Helper.cs"),
        "class Helper {}\n",
    )
    .unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &components, &policy, &providers)
            .is_err()
    );
    let fresh = read(&root);
    let current = fresh
        .admit(&components, &policy, &providers)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_ne!(
        admitted.proof().content_closure_sha256,
        current.proof().content_closure_sha256
    );
    assert!(
        fresh
            .verify_admission(admitted.proof(), &components, &policy, &providers)
            .is_err()
    );
    current
        .verify_inputs(root.path(), &components, &policy, &providers)
        .unwrap();
}

#[test]
fn retained_environment_metadata_preserves_its_owner_without_wrapper_field_leakage() {
    for container in ["settings_meta", "script_meta"] {
        marker_requires_classification(container);
    }
}

#[test]
fn ambiguous_markers_at_original_reference_sites_are_separately_blocked() {
    for container in [
        "settings",
        "importer",
        "asset",
        "settings_meta",
        "script_meta",
    ] {
        let receipt = projected(
            &fixture(container),
            json!({"$builtin":"canonical_unlit_quads", "$ref":format!("{GUID}:11400000"), "guid":GUID, "file_id":11_400_000}),
        );
        let proof = blocked(&receipt);
        let original = proof
            .reference_decisions
            .iter()
            .find(|d| {
                d.observation
                    .reference
                    .as_ref()
                    .is_some_and(|r| r.guid == GUID)
            })
            .unwrap();
        assert!(matches!(
            original.observation.resolution,
            ReferenceResolution::Unresolved { .. }
        ));
        assert!(
            proof
                .reference_decisions
                .iter()
                .any(|d| d.observation.reference.is_none()
                    && d.observation.raw_reference.get("$builtin").is_some()
                    && d.observation.raw_reference.get("$ref").is_some())
        );
    }
}
