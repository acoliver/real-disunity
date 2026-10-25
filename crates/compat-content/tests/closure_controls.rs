use serde_json::json;
use std::{fs, path::Path};
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_content};

const SCENE: &str = "Assets/Main.unity";
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("Assets/Scripts")).unwrap();
    fs::write(
        temp.path().join(SCENE),
        "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  shape: {x: 1, y: 2}\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(format!("{SCENE}.meta")),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    temp
}
fn imported(root: &Path) -> worldrunner_compat_content::ImportedContent {
    import_content(
        root,
        SCENE.as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap()
}

#[test]
fn helper_metadata_dependencies_are_transitive_and_payload_meta_drift_rejects_export() {
    let temp = fixture();
    fs::write(
        temp.path().join("Assets/Scripts/Helper.cs"),
        "class Helper {}\n",
    )
    .unwrap();
    fs::write(temp.path().join("Assets/Scripts/Helper.cs.meta"), "guid: 33333333333333333333333333333333\nImporter:\n  external: {fileID: 7, guid: 22222222222222222222222222222222}\n").unwrap();
    fs::write(temp.path().join("Assets/Bytes.bin"), b"original bytes").unwrap();
    fs::write(
        temp.path().join("Assets/Bytes.bin.meta"),
        "guid: 22222222222222222222222222222222\n",
    )
    .unwrap();
    for path in ["Assets/Bytes.bin", "Assets/Bytes.bin.meta"] {
        let snapshot = imported(temp.path());
        assert!(
            snapshot.sources().contains_key(path),
            "missing transitive {path}"
        );
        let original = fs::read(temp.path().join(path)).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n# changed\n");
        fs::write(temp.path().join(path), &changed).unwrap();
        assert!(
            snapshot
                .package_current(
                    temp.path(),
                    &worldrunner_asset_id::AssetVariant {
                        art_set: "original".into(),
                        quality: worldrunner_asset_id::QualityTier::Baseline,
                        platform_profile: "portable".into()
                    },
                    "test"
                )
                .is_err()
        );
        assert_eq!(fs::read(temp.path().join(path)).unwrap(), changed);
        fs::write(temp.path().join(path), original).unwrap();
    }
}

#[test]
fn closure_environment_bytes_and_entries_share_explicit_budgets() {
    let temp = fixture();
    fs::write(
        temp.path().join("Assets/Scripts/Large.cs"),
        vec![b' '; 2048],
    )
    .unwrap();
    for profile in [
        ImportProfile {
            max_file_bytes: 1024,
            ..ImportProfile::default()
        },
        ImportProfile {
            max_total_bytes: 1024,
            ..ImportProfile::default()
        },
        ImportProfile {
            max_catalog_entries: 2,
            ..ImportProfile::default()
        },
    ] {
        let result = import_content(
            temp.path(),
            SCENE.as_ref(),
            &profile,
            &ImportSidecar::default(),
        );
        assert!(matches!(
            result,
            Err(worldrunner_compat_content::ImportError::Limit(_))
        ));
    }
    for scene in ["../outside.unity", "/absolute.unity"] {
        assert!(
            import_content(
                temp.path(),
                scene.as_ref(),
                &ImportProfile::default(),
                &ImportSidecar::default()
            )
            .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn metadata_runtime_and_parent_directory_symlinks_are_rejected() {
    for relative in [
        "Assets/Main.unity.meta",
        "Assets/Scripts/Helper.cs",
        "ProjectSettings",
    ] {
        let temp = fixture();
        let path = temp.path().join(relative);
        let target = temp.path().join("actual");
        if path.exists() {
            fs::rename(&path, &target).unwrap();
        } else if relative == "ProjectSettings" {
            fs::create_dir(&target).unwrap();
        } else {
            fs::write(&target, "class Helper {}\n").unwrap();
        }
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(
            import_content(
                temp.path(),
                SCENE.as_ref(),
                &ImportProfile::default(),
                &ImportSidecar::default()
            )
            .is_err(),
            "followed {relative}"
        );
    }
}

#[test]
fn overlays_pin_identity_schema_shape_and_optional_current_source_hash() {
    let temp = fixture();
    let before = fs::read(temp.path().join(SCENE)).unwrap();
    let original = imported(temp.path());
    let base = json!({"guid":"11111111111111111111111111111111", "file_id":1,
        "property_path":"shape", "value":{"x":3,"y":4}, "reason":"Explicit test geometry",
        "precondition":{"schema_version":1,"class_id":1,"expected_value":{"x":1,"y":2},
        "source_sha256":original.graph().source_hashes[SCENE]}});
    for (pointer, value) in [
        ("/guid", json!("22222222222222222222222222222222")),
        ("/file_id", json!(9)),
        ("/precondition/schema_version", json!(2)),
        ("/precondition/class_id", json!(114)),
        ("/precondition/expected_value", json!([1, 2])),
        ("/precondition/source_sha256", json!("bad-hash")),
    ] {
        let mut patch = base.clone();
        *patch.pointer_mut(pointer).unwrap() = value;
        let sidecar = serde_json::from_value(json!({"field_overrides":[patch]})).unwrap();
        assert!(
            import_content(
                temp.path(),
                SCENE.as_ref(),
                &ImportProfile::default(),
                &sidecar
            )
            .is_err(),
            "accepted {pointer}"
        );
        assert_eq!(fs::read(temp.path().join(SCENE)).unwrap(), before);
    }
    let sidecar = serde_json::from_value(json!({"field_overrides":[base]})).unwrap();
    let changed = import_content(
        temp.path(),
        SCENE.as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap();
    assert_eq!(
        changed.graph().documents[0].fields["shape"],
        json!({"x":3,"y":4})
    );
    assert_eq!(fs::read(temp.path().join(SCENE)).unwrap(), before);
}

#[test]
fn settings_and_scene_share_the_total_document_budget() {
    let temp = fixture();
    fs::create_dir(temp.path().join("ProjectSettings")).unwrap();
    for name in ["One", "Two"] {
        fs::write(
            temp.path().join(format!("ProjectSettings/{name}.asset")),
            "%YAML 1.1\n--- !u!129 &1\nSettings:\n  value: 1\n",
        )
        .unwrap();
    }
    let profile = ImportProfile {
        max_documents: 2,
        ..ImportProfile::default()
    };
    let result = import_content(
        temp.path(),
        SCENE.as_ref(),
        &profile,
        &ImportSidecar::default(),
    );
    assert!(
        matches!(
            result,
            Err(worldrunner_compat_content::ImportError::Limit(_))
        ),
        "settings bypassed the cumulative document budget"
    );
}
