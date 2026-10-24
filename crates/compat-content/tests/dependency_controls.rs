mod dependency_support;
use dependency_support::*;
use std::fs;
use worldrunner_compat_content::*;

#[test]
fn editor_rule_rejects_wrong_class_file_location_hash_reference_and_kind() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let original = editor_policy(&receipt);
    for control in 0..10 {
        let mut policy = original.clone();
        let rule = &mut policy.rules[0];
        match control {
            0 => rule.source_sha256 = "0".repeat(64),
            1 => rule.resource_kind = ResourceKind::Material,
            2 => rule.expected_reference.file_id += 1,
            3 => rule.expected_reference.reference_type = Some(3),
            4 => rule.expected_reference.guid = "f".repeat(32),
            5 => rule.locator.source_path = "ProjectSettings/Other.asset".into(),
            6 => rule.locator.field.push(FieldSegment::Index(0)),
            7 => {
                if let OwnerIdentity::SettingsDocument { class_id, .. } = &mut rule.locator.owner {
                    *class_id = 30;
                }
            }
            8 => {
                if let OwnerIdentity::SettingsDocument { file_id, .. } = &mut rule.locator.owner {
                    *file_id = 2;
                }
            }
            _ => rule.evidence[0].sha256 = "f".repeat(64),
        }
        assert!(
            receipt
                .admit(
                    &AdmissionPolicy::default(),
                    &policy,
                    &TrustedProviderInventory::new()
                )
                .is_err(),
            "control {control}"
        );
    }
}

#[test]
fn newly_reviewed_runtime_or_unknown_urp_record_cannot_be_editor_exempted() {
    for text in [
        EDITOR.replace("!u!114 &1", "!u!30 &1"),
        EDITOR.replace("&1", "&2"),
        EDITOR.replace("13964", "11500000").replace(
            "0000000000000000e000000000000000",
            "247994e1f5a72c2419c26a37e9334c01",
        ),
    ] {
        let root = project(&text);
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
    let root = project(EDITOR);
    let mut scene = fs::read_to_string(root.path().join("Assets/Main.unity")).unwrap();
    scene.push_str("--- !u!114 &99\nMonoBehaviour:\n  m_Script: {fileID: 13964, guid: 0000000000000000e000000000000000, type: 0}\n");
    fs::write(root.path().join("Assets/Main.unity"), scene).unwrap();
    let receipt = read(&root);
    let err = receipt
        .admit(
            &AdmissionPolicy::default(),
            &editor_policy(&receipt),
            &TrustedProviderInventory::new(),
        )
        .err()
        .unwrap();
    assert!(
        err.proof
            .reference_decisions
            .iter()
            .any(|d| d.observation.locator.source_path == "Assets/Main.unity"
                && d.classification == DependencyClassification::Blocked)
    );
}

#[test]
fn proof_seal_detects_mutated_classifications_even_when_id_is_reused() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let policy = editor_policy(&receipt);
    let providers = TrustedProviderInventory::new();
    let admitted = receipt
        .admit(&AdmissionPolicy::default(), &policy, &providers)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut forged = admitted.proof().clone();
    forged.reference_decisions.clear();
    assert!(
        receipt
            .verify_admission(&forged, &AdmissionPolicy::default(), &policy, &providers)
            .is_err()
    );
}

#[test]
fn drift_symlink_and_fresh_compatible_sources_are_distinct() {
    let root = project(EDITOR);
    let receipt = read(&root);
    let policy = editor_policy(&receipt);
    let providers = TrustedProviderInventory::new();
    let admitted = receipt
        .admit(&AdmissionPolicy::default(), &policy, &providers)
        .unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir(root.path().join("Assets/Scripts")).unwrap();
    fs::write(
        root.path().join("Assets/Scripts/Helper.cs"),
        "class Helper {}\n",
    )
    .unwrap();
    assert!(admitted.verify_current(root.path()).is_err());
    let fresh = read(&root);
    assert!(
        fresh
            .admit(&AdmissionPolicy::default(), &policy, &providers)
            .is_ok()
    );
    assert!(
        fresh
            .verify_admission(
                admitted.proof(),
                &AdmissionPolicy::default(),
                &policy,
                &providers
            )
            .is_err()
    );
    #[cfg(unix)]
    {
        fs::remove_file(root.path().join(SETTINGS)).unwrap();
        std::os::unix::fs::symlink(
            root.path().join("Assets/Main.unity"),
            root.path().join(SETTINGS),
        )
        .unwrap();
        assert!(
            import_receipt(
                root.path(),
                "Assets/Main.unity".as_ref(),
                &ImportProfile::default(),
                &ImportSidecar::default()
            )
            .is_err()
        );
    }
}
