//! A script component of a player scene always carries the execution order its script stores,
//! 0 included, because a player's order replaces the class's attribute.

use std::path::Path;

use worldrunner_unity_import::{plan::PlanComponent, player_plan::plan_player_scene};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    object::PPtr,
    script_data::{ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 4] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
];

fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

fn class(class_id: ClassId) -> SerializedType {
    SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: (class_id == ClassId::MONO_BEHAVIOUR).then_some([7; 16]),
        old_type_hash: [0; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    }
}

fn object(path_id: i64, class_id: ClassId, data: Vec<u8>) -> ObjectData {
    let type_index = TYPES.iter().position(|&c| c == class_id).unwrap();
    ObjectData {
        path_id,
        type_index: i32::try_from(type_index).unwrap(),
        data,
    }
}

fn game_object() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(&[ptr(3), ptr(4)])
        .unwrap()
        .u32(0)
        .string("Setup")
        .unwrap()
        .u16(0)
        .u8(1);
    w.finish()
}

fn transform() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(2)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(&[]).unwrap().pptr(ptr(0));
    w.finish()
}

fn behaviour() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(2))
        .u8(1)
        .align()
        .pptr(ptr(5))
        .string("")
        .unwrap();
    w.finish()
}

/// The script `Setup` of `Example.Game`, with the execution order a player stores for it.
fn script(execution_order: i32) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Setup")
        .unwrap()
        .i32(execution_order)
        .bytes(&[0; 16]);
    w.string("Setup")
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

fn write_scene(dir: &Path, execution_order: i32) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects: vec![
            object(2, ClassId::GAME_OBJECT, game_object()),
            object(3, ClassId::TRANSFORM, transform()),
            object(4, ClassId::MONO_BEHAVIOUR, behaviour()),
            object(5, ClassId::MONO_SCRIPT, script(execution_order)),
        ],
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join("level0"), contents.write().unwrap()).unwrap();
}

fn plan_with_order(execution_order: i32) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path(), execution_order);
    let layouts = ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Setup".to_owned(),
        value_type: false,
        fields: Vec::new(),
    }]);
    let plan = plan_player_scene(dir.path(), "level0", &layouts).unwrap();
    assert!(matches!(
        plan.objects[0].components[..],
        [PlanComponent::Script { .. }]
    ));
    serde_json::to_value(&plan.objects[0].components[0]).unwrap()
}

#[test]
fn a_script_component_carries_the_order_its_script_stores() {
    let component = plan_with_order(-200);
    assert_eq!(component["execution_order"], -200);
}

#[test]
fn an_order_of_zero_is_carried_so_it_replaces_a_class_attribute() {
    let component = plan_with_order(0);
    assert_eq!(component["execution_order"], 0, "{component}");
}
