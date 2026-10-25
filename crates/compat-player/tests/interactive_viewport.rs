use compat_kit::fixture;
use std::path::Path;
use worldrunner::compat_content::{ImportProfile, ImportSidecar, ImportedScene, import_project};

fn snake_scene() -> (serde_json::Value, ImportedScene) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let selected = fixture::load(&root, "snake-interactive").unwrap();
    let source = fixture::source_root(&selected).unwrap();
    let profile: ImportProfile =
        serde_json::from_value(selected.metadata["import_profile"].clone()).unwrap();
    let sidecar: ImportSidecar =
        serde_json::from_value(selected.metadata["compatibility_sidecar"].clone()).unwrap();
    let scene = import_project(
        &source,
        Path::new(selected.metadata["scene"].as_str().unwrap()),
        &profile,
        &sidecar,
    )
    .unwrap();
    (selected.metadata, scene)
}

#[test]
fn interactive_snake_viewport_contains_the_entire_imported_board() {
    let (metadata, scene) = snake_scene();
    let viewport: [u32; 2] = metadata.get("viewport").map_or([960, 720], |size| {
        serde_json::from_value(size.clone()).unwrap()
    });
    let camera = scene
        .components
        .iter()
        .find(|component| component.native_kind.as_deref() == Some("Camera"))
        .unwrap();
    let owner = scene
        .nodes
        .iter()
        .find(|node| Some(&node.id) == camera.object.as_ref())
        .unwrap();
    let config = scene
        .documents
        .iter()
        .find(|document| document.fields.get("gridSize").is_some())
        .unwrap();
    let cell = config.fields["cellSize"].as_f64().unwrap();
    let half_height = camera.native_fields["orthographic_size"].as_f64().unwrap();
    let half_width = half_height * f64::from(viewport[0]) / f64::from(viewport[1]);
    for (axis, half_extent, dimension) in [(0, half_width, "x"), (1, half_height, "y")] {
        let camera_center = owner.transform.local_position[axis];
        let board_min = -0.5 * cell;
        let board_max = (config.fields["gridSize"][dimension].as_f64().unwrap() - 0.5) * cell;
        assert!(
            camera_center - half_extent <= board_min && camera_center + half_extent >= board_max,
            "viewport {viewport:?} crops imported board on {dimension}: camera center {camera_center}, half extent {half_extent}, board [{board_min}, {board_max}]"
        );
    }
}
