//! Resource game objects planned as prefabs: a scene plan of template objects per prefab.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::player_resources::{
    ResourceAsset, ResourcesPlan, plan_player_resources,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 6] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::TEXT_ASSET,
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
        script_id: (class_id == ClassId::MONO_BEHAVIOUR).then_some([7; 16]),
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
            .map(|path| External {
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

/// Resource objects of `resources.assets` (path IDs in the player):
/// 10 Crate (root) with 11 transform, script 14 and child 12 Lid with transform 13;
/// 30 Broken, whose script has no layout; 40 Hidden, which the container does not list;
/// 50 a text asset; 20 a script asset `Config`; 16, 17 and 18 scripts.
const CONTAINER: [(&str, i64); 5] = [
    ("prefabs/crate", 10),
    ("prefabs/broken", 30),
    ("prefabs/lid", 12),
    ("text/notes", 50),
    ("config/main", 20),
];

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(i32::try_from(CONTAINER.len()).unwrap());
    for (path, id) in CONTAINER {
        w.string(path).unwrap().pptr(ptr(1, id));
    }
    w.i32(0);
    w.finish()
}

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components)
        .unwrap()
        .u32(8)
        .string(name)
        .unwrap()
        .u16(0)
        .u8(1);
    w.finish()
}

fn transform(game_object: i64, father: PPtr, children: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[1.0, 2.0, 3.0]).floats(&[1.0; 3]);
    w.pptrs(children).unwrap().pptr(father);
    w.finish()
}

fn script(class: &str) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(class).unwrap().i32(0).bytes(&[0; 16]);
    w.string(class)
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

/// A script component of `owner` (0 for a script asset) with `script` and reference fields.
fn behaviour(owner: i64, script: i64, name: &str, targets: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner))
        .u8(1)
        .align()
        .pptr(ptr(0, script))
        .string(name)
        .unwrap();
    for &target in targets {
        w.pptr(target);
    }
    w.finish()
}

fn text_asset() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("notes").unwrap().byte_array(b"notes").unwrap();
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let object = |name: &str| ScriptField {
        name: name.to_owned(),
        ty: FieldType::Object("UnityEngine.Object".to_owned()),
    };
    let level = ScriptField {
        name: "level".to_owned(),
        ty: FieldType::Primitive(Primitive::I32),
    };
    ScriptLayouts::new(vec![
        ScriptType {
            key: "Example.Game:Example.Crate".to_owned(),
            value_type: false,
            fields: vec![
                level,
                object("lid"),
                object("notes"),
                object("other"),
                object("beyond"),
            ],
        },
        ScriptType {
            key: "Example.Game:Example.Config".to_owned(),
            value_type: false,
            fields: vec![object("hidden"), object("lid")],
        },
    ])
}

fn objects() -> Vec<ObjectData> {
    let crate_fields = [ptr(0, 12), ptr(0, 50), ptr(0, 40), ptr(1, 99)];
    let mut crate_behaviour = behaviour(10, 16, "", &[]);
    let mut level = ObjectWriter::new(false);
    level.i32(4);
    crate_behaviour.extend(level.finish());
    for target in crate_fields {
        let mut w = ObjectWriter::new(false);
        w.pptr(target);
        crate_behaviour.extend(w.finish());
    }
    vec![
        object(
            10,
            ClassId::GAME_OBJECT,
            game_object("Crate", &[ptr(0, 11), ptr(0, 14)]),
        ),
        object(
            11,
            ClassId::TRANSFORM,
            transform(10, ptr(0, 0), &[ptr(0, 13)]),
        ),
        object(12, ClassId::GAME_OBJECT, game_object("Lid", &[ptr(0, 13)])),
        object(13, ClassId::TRANSFORM, transform(12, ptr(0, 11), &[])),
        object(14, ClassId::MONO_BEHAVIOUR, crate_behaviour),
        object(16, ClassId::MONO_SCRIPT, script("Crate")),
        object(17, ClassId::MONO_SCRIPT, script("Nope")),
        object(18, ClassId::MONO_SCRIPT, script("Config")),
        object(
            20,
            ClassId::MONO_BEHAVIOUR,
            behaviour(0, 18, "main", &[ptr(0, 40), ptr(0, 12)]),
        ),
        object(
            30,
            ClassId::GAME_OBJECT,
            game_object("Broken", &[ptr(0, 31), ptr(0, 32)]),
        ),
        object(31, ClassId::TRANSFORM, transform(30, ptr(0, 0), &[])),
        object(32, ClassId::MONO_BEHAVIOUR, behaviour(30, 17, "", &[])),
        object(
            40,
            ClassId::GAME_OBJECT,
            game_object("Hidden", &[ptr(0, 41)]),
        ),
        object(41, ClassId::TRANSFORM, transform(40, ptr(0, 0), &[])),
        object(50, ClassId::TEXT_ASSET, text_asset()),
    ]
}

fn planned() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    write_file(dir.path(), "resources.assets", objects(), &[]);
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    (dir, plan)
}

fn read_plan(dir: &tempfile::TempDir, relative: &str) -> Json {
    let bytes = std::fs::read(dir.path().join("out").join(relative)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn a_resource_game_object_plans_as_a_prefab_with_its_own_plan_file() {
    let (dir, plan) = planned();
    let entries: Vec<_> = plan
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.class.as_str(), e.asset))
        .collect();
    assert_eq!(
        entries,
        [
            ("prefabs/crate", "GameObject", Some(0)),
            ("prefabs/broken", "GameObject", Some(1)),
            ("prefabs/lid", "GameObject", Some(2)),
            ("text/notes", "TextAsset", Some(3)),
            ("config/main", "MonoBehaviour", Some(4)),
        ]
    );
    let ResourceAsset::Prefab { name, plan: file } = &plan.assets[0] else {
        panic!("the crate is a prefab: {:?}", plan.assets[0])
    };
    assert_eq!((name.as_str(), file.as_str()), ("Crate", "prefabs/0.json"));
    let scene = read_plan(&dir, file);
    let objects = scene["objects"].as_array().unwrap();
    let summary: Vec<_> = objects
        .iter()
        .map(|o| {
            (
                o["name"].as_str().unwrap(),
                o["template"].as_bool().unwrap(),
                o["parent"].clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [("Crate", true, Json::Null), ("Lid", true, json!(0))]
    );
    assert_eq!(objects[0]["layer"], 8);
    assert_eq!(objects[0]["tag"], "Untagged");
    assert_eq!(objects[0]["position"], json!([1.0, 2.0, 3.0]));
}

#[test]
fn prefab_references_stay_in_the_prefab_or_name_resource_assets() {
    let (dir, plan) = planned();
    let scene = read_plan(&dir, "prefabs/0.json");
    let component = &scene["objects"][0]["components"][0];
    assert_eq!(component["class"], "Example.Crate");
    assert_eq!(
        component["fields"],
        json!({
            "level": "4",
            "lid": {"$object": 1},
            "notes": {"$asset": 3},
            "other": {"$asset": 5},
            "beyond": null,
        })
    );
    assert!(
        plan.differences.contains(
            &"prefab Crate: Crate.Example.Crate.beyond: references object 99 of file 1, which the plan does not hold"
                .to_owned()
        ),
        "{:?}",
        plan.differences
    );
}

#[test]
fn a_script_field_naming_a_prefab_root_plans_that_prefab_and_a_child_binds_null() {
    let (dir, plan) = planned();
    let ResourceAsset::Prefab { name, plan: file } = &plan.assets[5] else {
        panic!("Hidden is planned on demand: {:?}", plan.assets[5])
    };
    assert_eq!(name, "Hidden");
    assert_eq!(read_plan(&dir, file)["objects"][0]["name"], "Hidden");
    let ResourceAsset::Script { fields, .. } = &plan.assets[4] else {
        panic!("the config is a script asset: {:?}", plan.assets[4])
    };
    assert_eq!(
        read_plan(&dir, fields),
        json!({"hidden": {"$asset": 5}, "lid": null})
    );
    assert!(
        plan.differences
            .iter()
            .any(|d| d.starts_with("resource main.lid:") && d.contains("not a prefab root")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn a_prefab_that_does_not_plan_keeps_its_error_and_the_rest_still_plan() {
    let (_dir, plan) = planned();
    let ResourceAsset::UnplannedPrefab { name, error } = &plan.assets[1] else {
        panic!("Broken fails: {:?}", plan.assets[1])
    };
    assert_eq!(name, "Broken");
    assert!(error.contains("Example.Game:Example.Nope"), "{error}");
    let ResourceAsset::UnplannedPrefab { name, error } = &plan.assets[2] else {
        panic!("Lid is not a root: {:?}", plan.assets[2])
    };
    assert_eq!(name, "Lid");
    assert!(error.contains("not a prefab root"), "{error}");
    assert!(matches!(plan.assets[3], ResourceAsset::TextAsset { .. }));
    assert!(
        plan.differences
            .iter()
            .any(|d| d.starts_with("prefab Broken did not plan:")),
        "{:?}",
        plan.differences
    );
    assert!(
        plan.differences
            .contains(&"2 prefabs did not plan".to_owned()),
        "{:?}",
        plan.differences
    );
}
