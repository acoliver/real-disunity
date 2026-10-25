use serde_json::{Value, json};
use std::path::PathBuf;
use worldrunner_compat_content::{ImportProfile, ImportSidecar, ImportedScene, import_project};

fn imported(bounds: bool) -> ImportedScene {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = serde_json::from_slice::<ImportProfile>(
        &std::fs::read(base.join("profiles/breakout-source.json")).unwrap(),
    )
    .unwrap();
    let mut sidecar: Value =
        serde_json::from_slice(&std::fs::read(base.join("overlays/breakout.json")).unwrap())
            .unwrap();
    sidecar["tile_collider_fallbacks"] = if bounds {
        json!([{"guid":"8c9cfa26abfee488c85f1582747f6a02","file_id":2_096_728_906,
            "geometry":"sprite_bounds","reason":"Original sprite has no authored physicsShape; explicit rect/PPU/pivot bounds, no alpha tracing."}])
    } else {
        json!([])
    };
    import_project(
        &worldrunner_original_games::checkout(worldrunner_original_games::Game::Breakout).unwrap(),
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &profile,
        &serde_json::from_value::<ImportSidecar>(sidecar).unwrap(),
    )
    .unwrap()
}

#[test]
fn original_tile_collision_has_48_cells_resolved_material_and_labeled_bounds() {
    let graph = imported(true);
    let collider = graph
        .components
        .iter()
        .find(|c| c.kind == "TilemapCollider2D")
        .unwrap();
    assert_eq!(collider.native_kind.as_deref(), Some("TilemapCollider2D"));
    let fields = &collider.native_fields;
    assert_eq!(fields["cell_size"], json!([2., 1.]));
    assert_eq!(fields["cell_gap"], json!([0., 0.]));
    assert_eq!(fields["tile_anchor"], json!([0.5, 0.5]));
    assert_eq!(fields["offset"], json!([0., 0.]));
    assert_eq!(fields["is_trigger"], false);
    assert_eq!(fields["material"]["bounciness"].as_f64(), Some(1.));
    assert_eq!(fields["material"]["friction"].as_f64(), Some(0.));
    let cells = fields["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 48);
    assert!(cells.iter().any(|c| c["position"] == json!([-5, -6])));
    verify_bounds_cells(cells);
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "tile_collision_geometry_fallback"
                && d.file_id == Some(collider.source.file_id)
                && d.message.contains("sprite_bounds")
                && d.message.contains("48"))
    );
    assert!(
        !graph
            .diagnostics
            .iter()
            .any(|d| d.code == "unsupported_component"
                && d.component.as_deref() == Some("TilemapCollider2D"))
    );
}

fn verify_bounds_cells(cells: &[Value]) {
    for cell in cells {
        assert_eq!(cell["collider_type"], "sprite");
        assert_eq!(cell["sprite_size"], json!([2., 1.]));
        assert_eq!(cell["sprite_pivot"], json!([0.5, 0.5]));
        assert_eq!(cell["physics_shapes"], json!([]));
        assert_eq!(cell["matrix"], json!([1., 0., 0., 1., 0., 0.]));
    }
}

#[test]
fn missing_authored_sprite_shapes_requires_explicit_bounds_selection() {
    let graph = imported(false);
    let collider = graph
        .components
        .iter()
        .find(|c| c.kind == "TilemapCollider2D")
        .unwrap();
    assert!(collider.native_kind.is_none());
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid_tile_collider"
                && d.message.contains("physicsShape")
                && d.message.contains("explicit"))
    );
}

#[test]
fn brick_stale_fields_are_explicitly_excluded_from_binding_and_preserved_in_source() {
    let graph = imported(true);
    let brick = graph
        .components
        .iter()
        .find(|c| c.script.as_ref().is_some_and(|s| s.type_name == "Brick"))
        .unwrap();
    for field in ["powerUpChance", "widePlatformDuration"] {
        assert!(brick.fields.get(field).is_some());
        assert!(brick.script.as_ref().unwrap().fields.get(field).is_none());
        assert!(brick.native_fields.get(field).is_none());
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|d| d.code == "source_field_excluded"
                    && d.field.as_deref() == Some(field)
                    && d.message.contains("Brick"))
        );
    }
    assert_eq!(
        brick.script.as_ref().unwrap().fields["frameTime"],
        json!(0.05)
    );
}

#[test]
fn bounds_policy_requires_a_nonempty_geometry_fidelity_reason() {
    let sidecar = serde_json::from_value::<worldrunner_compat_content::ImportSidecar>(
        serde_json::json!({"tile_collider_fallbacks":[{
            "guid":"8c9cfa26abfee488c85f1582747f6a02", "file_id":2_096_728_906,
            "geometry":"sprite_bounds", "reason":" "
        }]}),
    )
    .unwrap();
    let result = worldrunner_compat_content::import_project(
        &worldrunner_original_games::checkout(worldrunner_original_games::Game::Breakout).unwrap(),
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &worldrunner_compat_content::ImportProfile::default(),
        &sidecar,
    );
    assert!(result.is_err(), "unlabeled geometry fallback must reject");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("tile_collider_fallbacks")
    );
}
