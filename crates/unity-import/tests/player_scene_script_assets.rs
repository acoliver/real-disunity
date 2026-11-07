//! Script assets (`ScriptableObject`s) that a player scene's script fields reference, planned as
//! scene assets with their own fields, so the objects the scene's scripts hold exist when the
//! scene loads.

use std::path::Path;

use serde_json::{Value, json};
use worldrunner_unity_import::{
    plan::{PlanComponent, ScenePlan},
    player_plan::plan_player_scene,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 4] = [
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
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

/// A `MonoBehaviour` of `owner` (0 for an asset) whose fields are `settings` or `count` and
/// `next`.
fn behaviour(
    owner: i64,
    script: PPtr,
    name: &str,
    fields: impl FnOnce(&mut ObjectWriter),
) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).u8(1).align().pptr(script);
    w.string(name).unwrap();
    fields(&mut w);
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let object = |name: &str, class: &str| ScriptField {
        name: name.to_owned(),
        ty: FieldType::Object(class.to_owned()),
    };
    ScriptLayouts::new(vec![
        ScriptType {
            key: "Example.Game:Example.Holder".to_owned(),
            value_type: false,
            fields: vec![
                object("settings", "Example.Settings"),
                object("again", "Example.Settings"),
                object("other", "Example.Settings"),
                object("owned", "Example.Settings"),
            ],
        },
        ScriptType {
            key: "Example.Game:Example.Settings".to_owned(),
            value_type: false,
            fields: vec![
                ScriptField {
                    name: "count".to_owned(),
                    ty: FieldType::Primitive(Primitive::I32),
                },
                object("next", "Example.Settings"),
            ],
        },
    ])
}

/// A scene whose root script holds the settings asset 10 twice, the asset 12 and the component
/// 13; asset 10 names asset 12 and asset 12 names asset 10.
fn write_player(dir: &Path) {
    let game_object = {
        let mut w = ObjectWriter::new(false);
        w.pptrs(&[ptr(0, 3), ptr(0, 4)])
            .unwrap()
            .u32(0)
            .string("Root")
            .unwrap()
            .u16(0)
            .u8(1);
        w.finish()
    };
    let transform = {
        let mut w = ObjectWriter::new(false);
        w.pptr(ptr(0, 2)).floats(&[0.0, 0.0, 0.0, 1.0]);
        w.floats(&[0.0; 3]).floats(&[1.0; 3]);
        w.pptrs(&[]).unwrap().pptr(ptr(0, 0));
        w.finish()
    };
    let holder = behaviour(2, ptr(0, 5), "", |w| {
        w.pptr(ptr(1, 10)).pptr(ptr(1, 10)).pptr(ptr(1, 12));
        w.pptr(ptr(1, 13));
    });
    write_file(
        dir,
        "level0",
        vec![
            object(2, ClassId::GAME_OBJECT, game_object),
            object(3, ClassId::TRANSFORM, transform),
            object(4, ClassId::MONO_BEHAVIOUR, holder),
            object(5, ClassId::MONO_SCRIPT, script("Holder")),
        ],
        &["sharedassets0.assets"],
    );
    let settings = |count: i32, next: i64| {
        behaviour(0, ptr(0, 11), &format!("settings {count}"), |w| {
            w.i32(count).pptr(ptr(0, next));
        })
    };
    let owned = behaviour(2, ptr(0, 11), "owned", |w| {
        w.i32(9).pptr(ptr(0, 0));
    });
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(10, ClassId::MONO_BEHAVIOUR, settings(1, 12)),
            object(12, ClassId::MONO_BEHAVIOUR, settings(2, 10)),
            object(13, ClassId::MONO_BEHAVIOUR, owned),
            object(11, ClassId::MONO_SCRIPT, script("Settings")),
        ],
        &[],
    );
}

fn planned() -> ScenePlan {
    let dir = tempfile::tempdir().unwrap();
    write_player(dir.path());
    plan_player_scene(dir.path(), "level0", &layouts()).unwrap()
}

fn assets(plan: &ScenePlan) -> Vec<Value> {
    serde_json::to_value(&plan.assets)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn script_fields(plan: &ScenePlan) -> &Value {
    let [PlanComponent::Script { fields, .. }] = &plan.objects[0].components[..] else {
        panic!("the root has one script: {:?}", plan.objects[0].components)
    };
    fields
}

#[test]
fn a_referenced_script_asset_is_a_scene_asset_with_its_class_name_and_fields() {
    let plan = planned();
    let assets = assets(&plan);
    assert_eq!(assets.len(), 2, "{assets:?}");
    assert_eq!(assets[0]["type"], "Script");
    assert_eq!(assets[0]["class"], "Example.Settings");
    assert_eq!(assets[0]["assembly"], "Example.Game");
    assert_eq!(assets[0]["name"], "settings 1");
    assert_eq!(
        assets[0]["fields"],
        json!({"count": "1", "next": {"$asset": 1}})
    );
}

#[test]
fn assets_that_name_each_other_plan_once_each() {
    let plan = planned();
    let assets = assets(&plan);
    assert_eq!(assets[1]["name"], "settings 2");
    assert_eq!(
        assets[1]["fields"],
        json!({"count": "2", "next": {"$asset": 0}})
    );
}

#[test]
fn the_scripts_fields_refer_to_the_assets_and_a_shared_asset_is_one_entry() {
    let plan = planned();
    assert_eq!(
        *script_fields(&plan),
        json!({
            "settings": {"$asset": 0},
            "again": {"$asset": 0},
            "other": {"$asset": 1},
            "owned": null,
        })
    );
}

#[test]
fn a_script_component_of_an_object_is_not_an_asset_and_binds_null() {
    let plan = planned();
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("Root.Example.Holder.owned:")),
        "{:?}",
        plan.differences
    );
    assert_eq!(
        plan.differences
            .iter()
            .filter(|line| line.contains("settings")
                || line.contains("again")
                || line.contains(".other"))
            .count(),
        0,
        "{:?}",
        plan.differences
    );
}
