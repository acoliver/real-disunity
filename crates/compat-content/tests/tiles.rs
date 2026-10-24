use std::path::PathBuf;
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};

#[test]
fn original_tilemap_resolves_48_authored_cells_sprite_indices_and_grid_metadata() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.work/games/breakout");
    let graph = import_project(
        &root,
        "Assets/Scenes/SampleScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let tilemap = graph
        .components
        .iter()
        .find(|c| c.kind == "Tilemap")
        .unwrap();
    let fields = &tilemap.native_fields;
    let cells = fields["tiles"]
        .as_array()
        .expect("host-readable native tile cells");
    assert_eq!(cells.len(), 48);
    assert!(
        cells
            .iter()
            .any(|c| c["position"] == serde_json::json!([-5, -6, 0]))
    );
    assert!(
        cells
            .iter()
            .all(|c| c["sprite"]["file_id"] == -2_105_052_758_i64)
    );
    assert!(
        cells
            .iter()
            .all(|c| c["tile"]["$ref"] == "a35e1686666b1e76bbc0b49f58ec4f32:11400000")
    );
    assert_eq!(fields["grid"]["m_CellSize"]["x"], 2);
    assert_eq!(fields["tile_anchor"], serde_json::json!([0.5, 0.5, 0.]));
    assert_eq!(fields["renderer"]["m_SortingOrder"], 0);
    assert_eq!(fields["m_Tiles"].as_array().unwrap().len(), 48);
}
