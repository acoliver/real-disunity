//! Sorting groups in player scenes and prefabs: the stored values reach the plan, and a group
//! that does not decode plans the defaults and is listed once.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::{
    player_plan::plan_player_scene,
    player_resources::{ResourceAsset, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components).unwrap().u32(0).string(name).unwrap();
    w.u16(0).u8(1);
    w.finish()
}

fn transform(game_object: i64, father: PPtr, children: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(children).unwrap().pptr(father);
    w.finish()
}

const TYPES: [ClassId; 4] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::SORTING_GROUP,
    ClassId::RESOURCE_MANAGER,
];

fn object(path_id: i64, class_id: ClassId, data: Vec<u8>) -> ObjectData {
    let type_index = TYPES.iter().position(|&c| c == class_id).unwrap();
    ObjectData {
        path_id,
        type_index: i32::try_from(type_index).unwrap(),
        data,
    }
}

fn write_file(dir: &Path, name: &str, objects: Vec<ObjectData>, externals: &[&str]) {
    let class = |class_id: ClassId| SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: None,
        old_type_hash: [0; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    };
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects,
        scripts: Vec::new(),
        externals: externals
            .iter()
            .map(|path| worldrunner_unity_serialized::External {
                guid: [0; 16],
                kind: 0,
                path: (*path).into(),
            })
            .collect(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join(name), contents.write().unwrap()).unwrap();
}

/// A group as the layout stores it: owner, enabled, layer id, layer index, order, sort at root.
fn group_bytes(enabled: bool, layer_id: i32, order: i16, sort_at_root: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 2)).u8(u8::from(enabled)).align();
    w.i32(layer_id).i16(0).i16(order).u8(u8::from(sort_at_root));
    w.align();
    w.finish()
}

fn widget(group: Vec<u8>) -> Vec<ObjectData> {
    vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Widget", &[ptr(0, 3), ptr(0, 4)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[])),
        object(4, ClassId::SORTING_GROUP, group),
    ]
}

fn planned_group(dir: &Path) -> (Json, Vec<String>) {
    let layouts = ScriptLayouts::new(Vec::new());
    let plan = plan_player_scene(dir, "level0", &layouts).unwrap();
    let json = serde_json::to_value(&plan.objects[0].components[0]).unwrap();
    (json, plan.differences)
}

#[test]
fn a_scene_plans_the_stored_sorting_group_values() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "level0",
        widget(group_bytes(false, 0, -12, true)),
        &[],
    );
    let (json, differences) = planned_group(dir.path());
    assert_eq!(
        json,
        json!({
            "type": "SortingGroup",
            "enabled": false,
            "sorting_layer_id": 0,
            "sorting_order": -12,
            "sort_at_root": true,
        })
    );
    assert_eq!(differences, Vec::<String>::new());
}

#[test]
fn the_layer_id_and_a_positive_order_are_planned_as_stored() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "level0",
        widget(group_bytes(true, 41, 300, false)),
        &[],
    );
    let (json, _) = planned_group(dir.path());
    assert_eq!(json["sorting_layer_id"], 41);
    assert_eq!(json["sorting_order"], 300);
    assert_eq!(json["enabled"], true);
    assert_eq!(json["sort_at_root"], false);
}

#[test]
fn a_group_that_does_not_decode_plans_the_defaults_and_is_listed_once() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "level0", widget(vec![1, 2, 3]), &[]);
    let (json, differences) = planned_group(dir.path());
    assert_eq!(
        json,
        json!({
            "type": "SortingGroup",
            "enabled": true,
            "sorting_layer_id": 0,
            "sorting_order": 0,
            "sort_at_root": false,
        })
    );
    assert_eq!(differences.len(), 1, "{differences:?}");
    assert!(
        differences[0].starts_with("Widget: SortingGroup does not decode"),
        "{differences:?}"
    );
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(1).string("prefabs/widget").unwrap().pptr(ptr(1, 2));
    w.i32(0);
    w.finish()
}

#[test]
fn a_prefab_plans_its_sorting_group() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    write_file(
        dir.path(),
        "resources.assets",
        widget(group_bytes(true, 0, 5, true)),
        &[],
    );
    let out = dir.path().join("out");
    let plan = plan_player_resources(dir.path(), &ScriptLayouts::new(Vec::new()), &out).unwrap();
    let ResourceAsset::Prefab { plan: file, .. } = &plan.assets[0] else {
        panic!("the widget is a prefab: {:?}", plan.assets[0])
    };
    let scene: Json = serde_json::from_slice(&std::fs::read(out.join(file)).unwrap()).unwrap();
    let group = &scene["objects"][0]["components"][0];
    assert_eq!(group["type"], "SortingGroup");
    assert_eq!(group["sorting_order"], 5);
    assert_eq!(group["sort_at_root"], true);
}
