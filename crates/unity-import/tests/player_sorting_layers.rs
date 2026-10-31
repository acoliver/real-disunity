//! The sorting layers of a player's tag manager reach the scene plan in drawing order.

use std::path::Path;

use serde_json::json;
use worldrunner_unity_import::player_plan::plan_player_scene;
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

fn write_file(dir: &Path, name: &str, objects: Vec<ObjectData>) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: vec![SerializedType {
            class_id: ClassId::TAG_MANAGER,
            stripped: false,
            script_type_index: -1,
            script_id: None,
            old_type_hash: [0; 16],
            type_tree: None,
            dependencies: Vec::new(),
            script_class: None,
        }],
        objects,
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join(name), contents.write().unwrap()).unwrap();
}

/// A tag manager with no tags or layers and the given sorting layers, in stored order.
fn tag_manager(sorting_layers: &[(&str, u32)]) -> ObjectData {
    let mut w = ObjectWriter::new(false);
    w.i32(0).i32(0);
    w.i32(i32::try_from(sorting_layers.len()).unwrap());
    for &(name, id) in sorting_layers {
        w.string(name).unwrap().u32(id);
    }
    w.i32(0);
    ObjectData {
        path_id: 1,
        type_index: 0,
        data: w.finish(),
    }
}

fn plan(sorting_layers: Option<&[(&str, u32)]>) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    if let Some(layers) = sorting_layers {
        write_file(dir.path(), "globalgamemanagers", vec![tag_manager(layers)]);
    }
    write_file(dir.path(), "level0", vec![]);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    serde_json::to_value(&plan).unwrap()
}

#[test]
fn carries_the_sorting_layers_in_drawing_order_with_their_ids() {
    let json = plan(Some(&[
        ("Background", 7),
        ("Default", 0),
        ("Units", 4_000_000_000),
    ]));
    assert_eq!(
        json["sorting_layers"],
        json!([
            { "name": "Background", "id": 7 },
            { "name": "Default", "id": 0 },
            { "name": "Units", "id": -294_967_296_i64 },
        ])
    );
}

#[test]
fn leaves_the_sorting_layers_out_without_a_tag_manager() {
    assert!(plan(None).get("sorting_layers").is_none());
}

#[test]
fn leaves_the_sorting_layers_out_when_the_tag_manager_lists_none() {
    assert!(plan(Some(&[])).get("sorting_layers").is_none());
}
