//! Script assets that a resource only references, such as the profile a settings asset names or the
//! settings a profile lists, are planned with the asset that references them.

use std::path::Path;

use serde_json::{Value, json};
use worldrunner_unity_import::player_resources::{
    ResourceAsset, ResourcesPlan, plan_player_resources,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 5] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::GAME_OBJECT,
    ClassId::LIGHT,
];

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

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

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(1);
    w.string("config/main").unwrap().pptr(ptr(1, 3));
    w.i32(0);
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

/// A script object named `name` of the script at `script_id`, owned by `owner` (null for a
/// script asset), with a level and two object references.
fn behaviour(
    owner: PPtr,
    script_id: i64,
    name: &str,
    level: i32,
    notes: PPtr,
    light: PPtr,
) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(owner)
        .u8(1)
        .align()
        .pptr(ptr(0, script_id))
        .string(name)
        .unwrap()
        .i32(level)
        .pptr(notes)
        .pptr(light);
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    let object = || FieldType::Object("UnityEngine.Object".to_owned());
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Config".to_owned(),
        value_type: false,
        fields: vec![
            field("level", FieldType::Primitive(Primitive::I32)),
            field("notes", object()),
            field("light", object()),
        ],
    }])
}

const MAIN: i64 = 3;
const CONFIG_SCRIPT: i64 = 5;
const SUB: i64 = 6;
const COMPONENT: i64 = 7;
const UNLISTED: i64 = 9;
const UNLISTED_SCRIPT: i64 = 10;

/// `main` is the only listed resource. It references `sub`, a script asset that is not listed,
/// and a component of a game object. `sub` references an unlisted script asset whose class has
/// no layout, and `main` again.
fn planned() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    let none = ptr(0, 0);
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(
                MAIN,
                ClassId::MONO_BEHAVIOUR,
                behaviour(
                    none,
                    CONFIG_SCRIPT,
                    "main",
                    3,
                    ptr(0, SUB),
                    ptr(0, COMPONENT),
                ),
            ),
            object(CONFIG_SCRIPT, ClassId::MONO_SCRIPT, script("Config")),
            object(
                SUB,
                ClassId::MONO_BEHAVIOUR,
                behaviour(
                    none,
                    CONFIG_SCRIPT,
                    "sub",
                    9,
                    ptr(0, UNLISTED),
                    ptr(0, MAIN),
                ),
            ),
            object(
                COMPONENT,
                ClassId::MONO_BEHAVIOUR,
                behaviour(ptr(0, 8), CONFIG_SCRIPT, "", 1, none, none),
            ),
            object(8, ClassId::GAME_OBJECT, Vec::new()),
            object(
                UNLISTED,
                ClassId::MONO_BEHAVIOUR,
                behaviour(none, UNLISTED_SCRIPT, "unlisted", 1, none, none),
            ),
            object(UNLISTED_SCRIPT, ClassId::MONO_SCRIPT, script("Unlisted")),
        ],
        &[],
    );
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    (dir, plan)
}

fn fields(dir: &tempfile::TempDir, plan: &ResourcesPlan, index: usize) -> (String, Value) {
    let ResourceAsset::Script { name, fields, .. } = &plan.assets[index] else {
        panic!(
            "asset {index} is not a script asset: {:?}",
            plan.assets[index]
        )
    };
    let bytes = std::fs::read(dir.path().join("out").join(fields)).unwrap();
    (name.clone(), serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn plans_a_script_asset_that_a_resource_references() {
    let (dir, plan) = planned();
    assert_eq!(plan.assets.len(), 2, "{:?}", plan.assets);
    let (name, main) = fields(&dir, &plan, 0);
    assert_eq!(name, "main");
    assert_eq!(main["notes"], json!({"$asset": 1}));
    let (name, sub) = fields(&dir, &plan, 1);
    assert_eq!((name.as_str(), &sub["level"]), ("sub", &json!("9")));
}

#[test]
fn a_referenced_script_asset_can_reference_the_asset_that_referenced_it() {
    let (dir, plan) = planned();
    let (_, sub) = fields(&dir, &plan, 1);
    assert_eq!(sub["light"], json!({"$asset": 0}));
}

#[test]
fn a_script_component_of_a_game_object_is_not_a_script_asset() {
    let (dir, plan) = planned();
    let (_, main) = fields(&dir, &plan, 0);
    assert_eq!(main["light"], Value::Null);
    assert!(
        plan.differences.contains(&format!(
            "resource main.light: references object {COMPONENT} of file 0, which the plan does not hold"
        )),
        "{:?}",
        plan.differences
    );
}

#[test]
fn a_referenced_script_asset_that_does_not_decode_binds_null_and_names_why() {
    let (dir, plan) = planned();
    let (_, sub) = fields(&dir, &plan, 1);
    assert_eq!(sub["notes"], Value::Null);
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("resource sub.notes: ")
                && line.contains(&format!("object {UNLISTED}"))
                && line.contains("Example.Game:Example.Unlisted")
                && line.contains("no layout")),
        "{:?}",
        plan.differences
    );
}
