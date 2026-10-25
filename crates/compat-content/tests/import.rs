use std::path::PathBuf;
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};

fn root(game: &str) -> PathBuf {
    worldrunner_original_games::checkout(game.parse().unwrap()).unwrap()
}

#[test]
fn imports_original_scenes_and_reachable_prefabs_configs_and_scripts() {
    for (game, scene, minimum) in [
        ("snake", "GameScene", 6),
        ("breakout", "SampleScene", 10),
        ("pong", "Main", 18),
    ] {
        let graph = import_project(
            &root(game),
            format!("Assets/Scenes/{scene}.unity").as_ref(),
            &ImportProfile::default(),
            &ImportSidecar::default(),
        )
        .unwrap();
        assert!(
            graph.nodes.iter().filter(|n| !n.template).count() >= minimum,
            "{game}: prefab scene expansion missing"
        );
        assert!(
            graph.components.iter().any(|c| c.script.is_some()),
            "{game}: script binding missing"
        );
        assert!(
            graph.assets.iter().any(|a| a.path.ends_with(".prefab")),
            "{game}: prefab dependency missing"
        );
        assert!(graph.source_hashes.len() > 5);
    }
}

#[test]
fn snake_private_fields_resolve_config_asset_and_transform_prefab() {
    let g = import_project(
        &root("snake"),
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let config = g
        .components
        .iter()
        .filter_map(|c| c.script.as_ref())
        .find(|s| s.type_name == "GameConfig")
        .unwrap();
    assert_eq!(config.fields["gridSize"]["x"], 40);
    let view = g
        .components
        .iter()
        .find(|c| {
            c.script
                .as_ref()
                .is_some_and(|s| s.type_name == "SnakeView")
        })
        .unwrap();
    assert!(view.script.as_ref().unwrap().fields["segmentPrefab"]["$ref"].is_string());
    assert!(g.nodes.iter().any(|n| n.name == "Food" && !n.template));
}

#[test]
fn pong_2017_overrides_keep_axes_transforms_and_unityevent_target() {
    let g = import_project(
        &root("pong"),
        "Assets/Scenes/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let enemy = g
        .nodes
        .iter()
        .find(|n| n.name == "Enemy" && !n.template)
        .unwrap();
    assert!((enemy.transform.local_position[0] - 10.).abs() < f64::EPSILON);
    let player = g
        .components
        .iter()
        .find(|c| {
            c.object.as_ref() == Some(&enemy.id)
                && c.script.as_ref().is_some_and(|s| s.type_name == "Player")
        })
        .unwrap();
    assert_eq!(
        player.script.as_ref().unwrap().fields["axis"],
        "AlternateVertical"
    );
    let calls: Vec<_> = g
        .components
        .iter()
        .filter(|c| !c.source.instance.is_empty())
        .filter_map(|c| c.script.as_ref())
        .filter(|s| s.type_name == "Goal")
        .collect();
    assert_eq!(calls.len(), 2);
    for call in calls {
        let event = &call.fields["scoreEvent"]["m_PersistentCalls"]["m_Calls"][0];
        assert_eq!(event["m_MethodName"], "ScorePoint");
        let target = event["m_Target"]["$ref"].as_str().unwrap();
        assert!(g.components.iter().any(|c| {
            c.id == target
                && c.script
                    .as_ref()
                    .is_some_and(|s| s.type_name == "MatchManager")
        }));
    }
}

#[test]
fn profile_maps_dll_file_ids_to_distinct_managed_types_and_builtin_assets() {
    let profile: ImportProfile =
        serde_json::from_str(include_str!("../profiles/pong-source.json")).unwrap();
    let g = import_project(
        &root("pong"),
        "Assets/Scenes/Main.unity".as_ref(),
        &profile,
        &ImportSidecar::default(),
    )
    .unwrap();
    let texts: Vec<_> = g
        .components
        .iter()
        .filter(|c| c.native_kind.as_deref() == Some("Text"))
        .collect();
    assert_eq!(texts.len(), 4, "original and scene-instance Text bindings");
    assert!(
        texts
            .iter()
            .all(|c| c.fields["m_FontData"]["m_Font"]["$builtin"] == "host.builtin-font")
    );
    assert!(
        !g.diagnostics
            .iter()
            .any(|d| d.component.as_deref() == Some("SpriteRenderer")
                && d.code == "builtin_replacement_required")
    );
}

#[test]
fn nested_breakout_fields_and_source_local_missing_sprite_diagnostics_survive_import() {
    let g = import_project(
        &root("breakout"),
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let binding = g
        .components
        .iter()
        .filter_map(|c| c.script.as_ref())
        .find(|s| s.type_name == "BrickSpawner")
        .unwrap();
    assert!(
        binding.fields["brickTypes"][0]["breakFrames"]
            .as_array()
            .unwrap()
            .len()
            > 2
    );
    assert!(binding.fields["brickTypes"][0]["initialSprite"]["$ref"].is_string());
    let s = import_project(
        &root("snake"),
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    assert!(s.diagnostics.iter().any(|d| d.code == "missing_guid"
        && d.path == "Assets/Visuals/SneakSegment.prefab"
        && d.component.as_deref() == Some("SpriteRenderer")
        && d.field.as_deref() == Some("m_Sprite")
        && d.guid.len() == 32));
}

#[test]
fn rejects_missing_texture_subasset_id_with_original_script_field_location() {
    use worldrunner_compat_content::FieldOverride;
    let sidecar = ImportSidecar {
        field_overrides: vec![FieldOverride {
            guid: "8c9cfa26abfee488c85f1582747f6a02".into(),
            file_id: 478_907_698,
            property_path: "brickTypes.Array.data[0].initialSprite".into(),
            precondition: Some(worldrunner_compat_content::OverlayPrecondition {
                schema_version: 1,
                class_id: 114,
                expected_value: serde_json::json!({"fileID":8_482_151_852_852_566_296_i64,"guid":"e137195f441c425a79bb0d5da5dd2655","type":3}),
                source_sha256: None,
            }),
            value: serde_json::json!({"fileID":9_223_372_036_854_i64,"guid":"e137195f441c425a79bb0d5da5dd2655","type":3}),
            reason: "Exercise missing subasset admission against original sprite sheet".into(),
        }],
        ..ImportSidecar::default()
    };
    let g = import_project(
        &root("breakout"),
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap();
    assert!(g.diagnostics.iter().any(|d| d.code == "missing_file_id"
        && d.field.as_deref() == Some("brickTypes[0].initialSprite")));
}

#[test]
fn explicit_sidecars_substitute_missing_sprites_without_changing_source() {
    use worldrunner_compat_content::ReferenceSubstitution;
    let sidecar = ImportSidecar {
        reference_substitutions: vec![ReferenceSubstitution {
            guid: "311925a002f4447b3a28927169b83ea6".into(),
            file_id: Some(7_482_667_652_216_324_306),
            replacement: serde_json::json!({"$builtin":"fixture-authored-unit-square"}),
            reason: "Explicit fixture-owned visual replacement; original sprite unavailable".into(),
        }],
        ..ImportSidecar::default()
    };
    let g = import_project(
        &root("snake"),
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap();
    assert!(
        !g.diagnostics
            .iter()
            .any(|d| d.code == "missing_guid" && d.field.as_deref() == Some("m_Sprite"))
    );
    assert!(
        g.components
            .iter()
            .filter(|c| c.kind == "SpriteRenderer")
            .all(|c| c.native_fields["sprite"]["$builtin"] == "fixture-authored-unit-square")
    );
}

#[test]
fn configured_stale_field_aliases_preserve_current_scene_override_values() {
    let profile: ImportProfile =
        serde_json::from_str(include_str!("../profiles/source-field-aliases.json")).unwrap();
    let b = import_project(
        &root("breakout"),
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &profile,
        &ImportSidecar::default(),
    )
    .unwrap();
    let ball = b
        .components
        .iter()
        .filter_map(|c| c.script.as_ref())
        .find(|s| s.type_name == "Ball")
        .unwrap();
    assert_eq!(ball.fields["speed"], 6);
    assert!(ball.fields.get("startSpeed").is_none());
    assert_eq!(
        b.nodes.iter().find(|n| n.name == "Ball").unwrap().tag,
        "Ball"
    );
    let p = import_project(
        &root("pong"),
        "Assets/Scenes/Main.unity".as_ref(),
        &profile,
        &ImportSidecar::default(),
    )
    .unwrap();
    let mut scores: Vec<_> = p
        .components
        .iter()
        .filter(|c| !c.source.instance.is_empty())
        .filter_map(|c| c.script.as_ref())
        .filter(|s| s.type_name == "Goal")
        .map(|s| s.fields["scoresTo"].as_i64().unwrap())
        .collect();
    scores.sort_unstable();
    assert_eq!(scores, vec![0, 1]);
}

#[test]
fn original_sprite_sheet_exposes_rect_pivot_scale_and_preserved_file_id_aliases() {
    let b = import_project(
        &root("breakout"),
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let sheet = b
        .assets
        .iter()
        .find(|a| a.guid == "e137195f441c425a79bb0d5da5dd2655")
        .unwrap();
    let sprite = sheet
        .sprites
        .iter()
        .find(|s| s.file_id == 6_081_121_525_837_946_721)
        .unwrap();
    assert_eq!(sprite.name, "platform-long");
    assert!(
        sprite
            .rect
            .iter()
            .zip([32., 227., 64., 14.])
            .all(|(actual, expected)| (actual - expected).abs() < f64::EPSILON)
    );
    assert!(
        sprite
            .pivot
            .iter()
            .all(|actual| (actual - 0.5).abs() < f64::EPSILON)
    );
    assert!((sprite.pixels_per_unit - 16.).abs() < f64::EPSILON);
    assert!(sheet.texture_size.is_some());
    assert!(sheet.sprites.iter().any(|s| s.file_id == 1_974_901_376));
    assert!(!b.diagnostics.iter().any(|d| d.code == "missing_file_id"));
}
