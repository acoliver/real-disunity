use std::fmt::Write as _;
use std::{
    fs,
    path::{Path, PathBuf},
};
use worldrunner_compat_content::{ImportError, ImportProfile, ImportSidecar, import_project};

fn original() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.work/games/snake")
}
fn copy_dir(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            copy_dir(&item.path(), &target.join(item.file_name()));
        } else {
            fs::copy(item.path(), target.join(item.file_name())).unwrap();
        }
    }
}
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_dir(&original().join("Assets"), &temp.path().join("Assets"));
    temp
}
fn import(root: &Path) -> worldrunner_compat_content::ImportedScene {
    import_project(
        root,
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap()
}

#[test]
fn changed_original_style_config_reimports_with_stable_identity_and_fresh_hashes() {
    let temp = fixture();
    let first = import(temp.path());
    let rel = "Assets/GameConfigs/GameConfig.asset";
    let text = fs::read_to_string(temp.path().join(rel)).unwrap();
    fs::write(
        temp.path().join(rel),
        text.replace("ticksPerSecond: 8", "ticksPerSecond: 12"),
    )
    .unwrap();
    let second = import(temp.path());
    assert_ne!(first.source_hashes[rel], second.source_hashes[rel]);
    assert_eq!(
        first.nodes.iter().map(|n| &n.id).collect::<Vec<_>>(),
        second.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    let cfg = second
        .components
        .iter()
        .filter_map(|c| c.script.as_ref())
        .find(|s| s.type_name == "GameConfig")
        .unwrap();
    assert_eq!(cfg.fields["ticksPerSecond"], 12);
    assert_eq!(import(temp.path()).source_hashes, second.source_hashes);
    assert_eq!(fs::read_to_string(original().join(rel)).unwrap(), text);
}

#[test]
fn unreachable_broken_assets_do_not_reject_the_selected_scene() {
    let temp = fixture();
    let first = import(temp.path());
    fs::write(
        temp.path().join("Assets/unselected.asset"),
        "%YAML malformed unrelated content",
    )
    .unwrap();
    fs::write(
        temp.path().join("Assets/unselected.asset.meta"),
        "fileFormatVersion: 2\nguid: abcdefabcdefabcdefabcdefabcdefab\n",
    )
    .unwrap();
    let second = import(temp.path());
    assert_eq!(first.source_hashes, second.source_hashes);
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
fn import_bounds_and_relative_scene_boundary_are_enforced() {
    for profile in [
        ImportProfile {
            max_file_bytes: 64,
            ..ImportProfile::default()
        },
        ImportProfile {
            max_assets: 1,
            ..ImportProfile::default()
        },
        ImportProfile {
            max_documents: 4,
            ..ImportProfile::default()
        },
        ImportProfile {
            max_total_bytes: 64,
            ..ImportProfile::default()
        },
    ] {
        let result = import_project(
            &original(),
            "Assets/Scenes/GameScene.unity".as_ref(),
            &profile,
            &ImportSidecar::default(),
        );
        assert!(matches!(result, Err(ImportError::Limit(_))));
    }
    assert!(
        import_project(
            &original(),
            "../escape.unity".as_ref(),
            &ImportProfile::default(),
            &ImportSidecar::default()
        )
        .is_err()
    );
}

#[test]
fn malformed_reference_ids_are_diagnosed_at_the_serialized_field() {
    let temp = fixture();
    let path = temp.path().join("Assets/Scenes/GameScene.unity");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        text.replace(
            "foodVisual: {fileID: 1594825253}",
            "foodVisual: {fileID: 1.25}",
        ),
    )
    .unwrap();
    let graph = import(temp.path());
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid_reference_id" && d.field.as_deref() == Some("foodVisual"))
    );
}

#[test]
fn cyclic_prefab_dependencies_hit_expansion_bound() {
    let temp = fixture();
    let path = temp.path().join("Assets/Visuals/FoodVisual.prefab");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("--- !u!1001 &987654321\nPrefabInstance:\n  m_SourcePrefab: {fileID: 100100000, guid: 9fc94e80fc628496aad0cc5e15516621, type: 3}\n  m_Modification:\n    m_Modifications: []\n");
    fs::write(path, text).unwrap();
    let profile = ImportProfile {
        max_prefab_depth: 3,
        ..ImportProfile::default()
    };
    assert!(matches!(
        import_project(
            temp.path(),
            "Assets/Scenes/GameScene.unity".as_ref(),
            &profile,
            &ImportSidecar::default()
        ),
        Err(ImportError::Limit(_))
    ));
}

#[test]
fn nested_original_prefab_expands_with_scoped_parent_and_overrides() {
    let temp = fixture();
    let read = |name: &str| {
        worldrunner_compat_content::yaml::parse_unity_yaml(
            &fs::read_to_string(temp.path().join(name)).unwrap(),
            100,
        )
        .unwrap()
    };
    let segment = read("Assets/Visuals/SneakSegment.prefab");
    let go = segment
        .iter()
        .find(|d| d.kind == "GameObject")
        .unwrap()
        .file_id;
    let transform = segment
        .iter()
        .find(|d| d.kind == "Transform")
        .unwrap()
        .file_id;
    let path = temp.path().join("Assets/Visuals/FoodVisual.prefab");
    let mut text = fs::read_to_string(&path).unwrap();
    write!(text, "--- !u!1001 &987654321\nPrefabInstance:\n  m_Modification:\n    m_TransformParent: {{fileID: 3756771195856016842}}\n    m_Modifications:\n    - target: {{fileID: {go}, guid: ec3fb882bbd2c4a23a601408bd39a7cf, type: 3}}\n      propertyPath: m_Name\n      value: Nested\n      objectReference: {{fileID: 0}}\n    - target: {{fileID: {transform}, guid: ec3fb882bbd2c4a23a601408bd39a7cf, type: 3}}\n      propertyPath: m_LocalPosition.x\n      value: 8\n      objectReference: {{fileID: 0}}\n  m_SourcePrefab: {{fileID: 100100000, guid: ec3fb882bbd2c4a23a601408bd39a7cf, type: 3}}\n").unwrap();
    fs::write(path, text).unwrap();
    let g = import(temp.path());
    let nested = g
        .nodes
        .iter()
        .find(|n| !n.template && n.name == "Nested")
        .unwrap();
    let food = g
        .nodes
        .iter()
        .find(|n| !n.template && n.name == "Food")
        .unwrap();
    assert_eq!(nested.parent.as_deref(), Some(food.id.as_str()));
    assert!((nested.transform.local_position[0] - 8.).abs() < f64::EPSILON);
    assert!(food.children.contains(&nested.id));
}

#[test]
fn removed_prefab_component_is_absent_from_instance_and_retained_in_template() {
    let temp = fixture();
    let path = temp.path().join("Assets/Scenes/GameScene.unity");
    let text = fs::read_to_string(&path).unwrap().replace("m_RemovedComponents: []", "m_RemovedComponents:\n    - {fileID: 8227976129249848278, guid: 9fc94e80fc628496aad0cc5e15516621, type: 3}");
    fs::write(path, text).unwrap();
    let g = import(temp.path());
    let food = g
        .nodes
        .iter()
        .find(|n| !n.template && n.name == "Food")
        .unwrap();
    assert!(
        !g.components
            .iter()
            .any(|c| c.kind == "SpriteRenderer" && c.object.as_ref() == Some(&food.id))
    );
    assert!(g.components.iter().any(|c| c.kind == "SpriteRenderer"
        && c.source.guid == "9fc94e80fc628496aad0cc5e15516621"
        && c.source.instance.is_empty()));
    assert!(
        !g.diagnostics.iter().any(|d| d.code == "missing_file_id"
            && d.field.as_deref() == Some("m_Component[1].component"))
    );
}

#[test]
fn unrelated_duplicate_guids_do_not_block_selected_scene_admission() {
    let temp = fixture();
    let first = import(temp.path());
    for file in ["unselected-a", "unselected-b"] {
        fs::write(
            temp.path().join(format!("Assets/{file}.asset")),
            "%YAML malformed unrelated content",
        )
        .unwrap();
        fs::write(
            temp.path().join(format!("Assets/{file}.asset.meta")),
            "fileFormatVersion: 2\nguid: abcdefabcdefabcdefabcdefabcdefab\n",
        )
        .unwrap();
    }
    assert_eq!(import(temp.path()).source_hashes, first.source_hashes);
}
