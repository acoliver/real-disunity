use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use worldrunner_compat_content::{ImportProfile, ImportSidecar, ImportedScene, import_project};

fn base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn source() -> PathBuf {
    worldrunner_original_games::checkout(worldrunner_original_games::Game::Breakout).unwrap()
}
fn import(root: &Path, overrides: Value) -> ImportedScene {
    let profile: ImportProfile = serde_json::from_slice(
        &std::fs::read(base().join("profiles/breakout-source.json")).unwrap(),
    )
    .unwrap();
    let sidecar = ImportSidecar {
        field_overrides: serde_json::from_value(overrides).unwrap(),
        ..ImportSidecar::default()
    };
    import_project(
        root,
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &profile,
        &sidecar,
    )
    .unwrap()
}
fn collider(scene: &ImportedScene) -> &worldrunner_compat_content::ImportedComponent {
    scene
        .components
        .iter()
        .find(|c| c.kind == "TilemapCollider2D")
        .unwrap()
}
fn tile_type(kind: i32) -> Value {
    json!({"guid":"a35e1686666b1e76bbc0b49f58ec4f32", "file_id":11_400_000,
        "property_path":"m_ColliderType", "precondition":{"schema_version":1,"class_id":114,"expected_value":1}, "value":kind, "reason":"Test selected source tile collider type"})
}
fn matrix(key: &str, value: f64) -> Value {
    json!({"guid":"8c9cfa26abfee488c85f1582747f6a02", "file_id":2_096_728_905,
        "property_path":format!("m_TileMatrixArray.Array.data[0].m_Data.{key}"),
        "precondition":{"schema_version":1,"class_id":1_839_735_485,"expected_value":i32::from(matches!(key, "e00" | "e11" | "e22" | "e33"))},
        "value":value, "reason":"Test source planar matrix mapping"})
}

#[test]
fn none_and_grid_tile_types_do_not_require_sprite_bounds_or_authored_shapes() {
    for (source_kind, native) in [(0, "none"), (2, "grid")] {
        let graph = import(&source(), json!([tile_type(source_kind)]));
        let c = collider(&graph);
        assert_eq!(c.native_kind.as_deref(), Some("TilemapCollider2D"));
        assert!(
            c.native_fields["cells"]
                .as_array()
                .unwrap()
                .iter()
                .all(|cell| cell["collider_type"] == native && cell["sprite_size"].is_null())
        );
        assert!(
            !graph
                .diagnostics
                .iter()
                .any(|d| d.code == "tile_collision_geometry_fallback")
        );
    }
}

#[test]
fn source_affine_keeps_rotation_scale_shear_and_translation_without_transposition() {
    let graph = import(
        &source(),
        json!([
            tile_type(2),
            matrix("e00", 0.),
            matrix("e10", 2.),
            matrix("e01", -3.),
            matrix("e11", 0.5),
            matrix("e03", 4.),
            matrix("e13", -5.)
        ]),
    );
    for cell in collider(&graph).native_fields["cells"].as_array().unwrap() {
        assert_eq!(cell["matrix"], json!([0., 2., -3., 0.5, 4., -5.]));
    }
}

#[test]
fn nonplanar_perspective_singular_and_unknown_collider_sources_fail_explicitly() {
    for change in [
        matrix("e20", 0.1),
        matrix("e02", 0.1),
        matrix("e23", 1.),
        matrix("e30", 0.1),
        matrix("e22", 2.),
        matrix("e33", 2.),
        matrix("e00", 0.),
        tile_type(3),
    ] {
        let overrides = if change["property_path"] == "m_ColliderType" {
            json!([change])
        } else {
            json!([tile_type(2), change])
        };
        let graph = import(&source(), overrides);
        assert!(collider(&graph).native_kind.is_none());
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|d| d.code == "invalid_tile_collider")
        );
    }
}

fn with_physics_shape(shape: Value) -> ImportedScene {
    let original = import(&source(), json!([]));
    let temp = tempfile::tempdir().unwrap();
    for path in original.source_hashes.keys() {
        let destination = temp.path().join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(source().join(path), destination).unwrap();
    }
    let path = temp
        .path()
        .join("Assets/Images/spritesheet-breakout.png.meta");
    let mut metadata: Value = serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let sprites = metadata["TextureImporter"]["spriteSheet"]["sprites"]
        .as_array_mut()
        .unwrap();
    let sprite = sprites.iter_mut().find(|s| s["name"] == "brick").unwrap();
    sprite["physicsShape"] = shape;
    std::fs::write(path, serde_yaml::to_string(&metadata).unwrap()).unwrap();
    import(temp.path(), json!([]))
}

#[test]
fn authored_pixel_physics_shape_uses_ppu_and_preserves_concave_area_as_convex_pieces() {
    let graph = with_physics_shape(json!([[{"x":-16,"y":-8},{"x":16,"y":-8},
        {"x":16,"y":0},{"x":0,"y":0},{"x":0,"y":8},{"x":-16,"y":8}]]));
    let c = collider(&graph);
    assert_eq!(c.native_kind.as_deref(), Some("TilemapCollider2D"));
    let shapes = c.native_fields["cells"][0]["physics_shapes"]
        .as_array()
        .unwrap();
    assert!(shapes.len() > 1);
    let mut area = 0.;
    for shape in shapes {
        let points = shape.as_array().unwrap();
        area += points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(a, b)| {
                a[0].as_f64().unwrap() * b[1].as_f64().unwrap()
                    - a[1].as_f64().unwrap() * b[0].as_f64().unwrap()
            })
            .sum::<f64>()
            / 2.;
    }
    assert!((area - 1.5).abs() < 1e-8);
    assert!(
        !graph
            .diagnostics
            .iter()
            .any(|d| d.code == "tile_collision_geometry_fallback")
    );
    assert_ne!(
        graph.source_hashes["Assets/Images/spritesheet-breakout.png.meta"],
        import(&source(), json!([])).source_hashes["Assets/Images/spritesheet-breakout.png.meta"]
    );
}

#[test]
fn malformed_authored_outline_is_rejected_without_bounds_substitution() {
    let graph = with_physics_shape(json!([[{"x":0,"y":0},{"x":16,"y":16},
        {"x":0,"y":16},{"x":16,"y":0}]]));
    assert!(collider(&graph).native_kind.is_none());
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid_tile_collider" && d.message.contains("physicsShape"))
    );
}
