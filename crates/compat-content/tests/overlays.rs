use std::path::PathBuf;
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project, script_census};

fn root(game: &str) -> PathBuf {
    worldrunner_original_games::checkout(game.parse().unwrap()).unwrap()
}

#[test]
fn authored_fallback_keeps_source_defect_and_exposes_pixels_and_sprite_metrics() {
    let sidecar: ImportSidecar = serde_json::from_value(serde_json::json!({
        "reference_substitutions": [{
            "guid":"311925a002f4447b3a28927169b83ea6", "file_id":7_482_667_652_216_324_306_i64,
            "replacement":{"$builtin":"authored.unit-square"}, "reason":"Absent original art"
        }],
        "fallback_sprites": [{
            "name":"authored.unit-square", "texture_size":[8,8], "rgba":[255,255,255,255],
            "sprite":{"file_id":7_482_667_652_216_324_306_i64,"name":"unit-square","rect":[0,0,8,8],
                "pivot":[0.5,0.5],"pixels_per_unit":8,"border":[0,0,0,0]},
            "reason":"User-authored solid square for missing original sprite"
        }]
    }))
    .unwrap();
    let graph = import_project(
        &root("snake"),
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap();
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "source_reference_missing"
                && d.field.as_deref() == Some("m_Sprite")
                && d.message.contains("311925a002f4447b3a28927169b83ea6"))
    );
    let fallback = &graph.fallback_sprites[0];
    assert_eq!(fallback.rgba8().unwrap(), vec![255; 8 * 8 * 4]);
    assert!((fallback.sprite.pixels_per_unit - 8.).abs() < f64::EPSILON);
    assert!(
        graph
            .components
            .iter()
            .filter(|c| c.kind == "SpriteRenderer")
            .all(|c| c.native_fields["sprite"]["file_id"] == 7_482_667_652_216_324_306_i64)
    );
}

#[test]
fn original_runtime_script_census_includes_unserialized_helpers_and_interfaces() {
    for (game, count, helper) in [
        ("snake", 8, "SnakeModel.cs"),
        ("breakout", 7, "PowerUp.cs"),
        ("pong", 20, "Interfaces/IInput.cs"),
    ] {
        let census = script_census(
            &root(game),
            "Assets/Scripts".as_ref(),
            &ImportProfile::default(),
        )
        .unwrap();
        assert_eq!(census.len(), count, "{game}");
        assert!(census.iter().any(|s| s.path.ends_with(helper)));
        assert!(
            census
                .iter()
                .all(|s| s.sha256.len() == 64 && s.byte_length > 0)
        );
    }
}
