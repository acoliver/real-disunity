use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::{plan::PlanComponent, player_plan::plan_player_scene};
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
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::LIGHT,
    ClassId::TAG_MANAGER,
];

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

fn game_object(name: &str, tag: u16, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components)
        .unwrap()
        .u32(8)
        .string(name)
        .unwrap()
        .u16(tag)
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

fn script() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Spinner").unwrap().i32(0).bytes(&[0; 16]);
    w.string("Spinner")
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

/// A `Spinner` on object 2: a speed, then references to the child object, its transform,
/// the root's light, and an object in another file.
fn behaviour() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 2))
        .u8(0)
        .align()
        .pptr(ptr(0, 5))
        .string("")
        .unwrap()
        .f32(2.5);
    for target in [ptr(0, 6), ptr(0, 7), ptr(0, 8), ptr(1, 99), ptr(0, 0)] {
        w.pptr(target);
    }
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    let object = || FieldType::Object("UnityEngine.Object".to_owned());
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Spinner".to_owned(),
        value_type: false,
        fields: vec![
            field("speed", FieldType::Primitive(Primitive::F32)),
            field("target", object()),
            field("pivot", object()),
            field("light", object()),
            field("material", object()),
            field("nothing", object()),
        ],
    }])
}

fn write_file(dir: &Path, name: &str, objects: Vec<ObjectData>) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects,
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join(name), contents.write().unwrap()).unwrap();
}

fn write_scene(dir: &Path) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects: vec![
            object(
                2,
                ClassId::GAME_OBJECT,
                game_object("Root", 5, &[ptr(0, 3), ptr(0, 8), ptr(0, 4)]),
            ),
            object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[ptr(0, 7)])),
            object(4, ClassId::MONO_BEHAVIOUR, behaviour()),
            object(5, ClassId::MONO_SCRIPT, script()),
            object(
                6,
                ClassId::GAME_OBJECT,
                game_object("Child", 0, &[ptr(0, 7)]),
            ),
            object(7, ClassId::TRANSFORM, transform(6, ptr(0, 3), &[])),
            object(8, ClassId::LIGHT, Vec::new()),
        ],
        scripts: Vec::new(),
        externals: vec![External {
            guid: [0; 16],
            kind: 0,
            path: "sharedassets0.assets".into(),
        }],
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join("level0"), contents.write().unwrap()).unwrap();
}

#[test]
fn plans_objects_scripts_and_references_and_lists_what_it_leaves_out() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &layouts()).unwrap();
    let names: Vec<_> = plan.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Root", "Child"]);
    let root = &plan.objects[0];
    assert_eq!((root.tag.as_str(), root.layer), ("MainCamera", 8));
    assert_eq!(plan.objects[1].parent, Some(0));
    // Positions widen from f32 exactly.
    assert_eq!(
        root.position.map(f64::to_bits),
        [1.0, 2.0, 3.0].map(f64::to_bits)
    );
    let [PlanComponent::Unsupported { class }, script] = &root.components[..] else {
        panic!("the root has a light and a script: {:?}", root.components)
    };
    assert_eq!(class, "Light");
    let PlanComponent::Script {
        class,
        assembly,
        enabled,
        fields,
        ..
    } = script
    else {
        panic!("the second component is the script")
    };
    assert_eq!(
        (class.as_str(), assembly.as_deref(), *enabled),
        ("Example.Spinner", Some("Example.Game"), false)
    );
    assert_eq!(
        *fields,
        json!({
            "speed": "2.5",
            "target": {"$object": 1},
            "pivot": {"$transform": 1},
            "light": Json::Null,
            "material": Json::Null,
            "nothing": Json::Null,
        })
    );
    assert_eq!(
        plan.differences,
        [
            "Root.Example.Spinner.light: references a Light component, which the engine does not build",
            "Root.Example.Spinner.material: references object 99 of file 1, which the plan does not hold",
        ]
    );
}

#[test]
fn names_the_script_whose_fields_do_not_decode() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let mut short = layouts();
    short.types[0].fields.pop();
    let short = ScriptLayouts::new(short.types);
    let error = plan_player_scene(dir.path(), "level0", &short).unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains("Root: script Example.Game:Example.Spinner"),
        "{text}"
    );
}

/// A `Spinner` on object 2 holding each kind of value the planner writes as text, then the
/// reference registry its `[SerializeReference]` field needs.
fn value_behaviour() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 2))
        .u8(1)
        .align()
        .pptr(ptr(0, 5))
        .string("")
        .unwrap();
    w.u64(u64::MAX)
        .u16(0xE9)
        .align()
        .f32(f32::NAN)
        .f32(f32::NEG_INFINITY);
    w.i32(2).i32(1).i32(-2).align();
    w.i64(-2).i32(2).i32(1).i64(-2);
    for _ in 0..3 {
        w.string("").unwrap();
    }
    w.finish()
}

fn value_layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    ScriptLayouts::new(vec![
        ScriptType {
            key: "Example.Game:Example.Spinner".to_owned(),
            value_type: false,
            fields: vec![
                field("big", FieldType::Primitive(Primitive::U64)),
                field("letter", FieldType::Primitive(Primitive::Char)),
                field("nan", FieldType::Primitive(Primitive::F32)),
                field("low", FieldType::Primitive(Primitive::F32)),
                field("pair", FieldType::Composite(1)),
                field("shape", FieldType::Reference("Example.IShape".to_owned())),
            ],
        },
        ScriptType {
            key: "Example.Game:Example.Pair".to_owned(),
            value_type: true,
            fields: vec![field(
                "values",
                FieldType::Array(Box::new(FieldType::Primitive(Primitive::I32))),
            )],
        },
    ])
}

#[test]
fn maps_user_tags_and_writes_every_value_kind_as_the_loader_reads_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut tags = ObjectWriter::new(false);
    tags.i32(1).string("Enemy").unwrap();
    for _ in 0..3 {
        tags.i32(0);
    }
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::TAG_MANAGER, tags.finish())],
    );
    write_file(
        dir.path(),
        "level0",
        vec![
            object(
                2,
                ClassId::GAME_OBJECT,
                game_object("Root", 20_000, &[ptr(0, 3), ptr(0, 4)]),
            ),
            object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[ptr(0, 7)])),
            object(4, ClassId::MONO_BEHAVIOUR, value_behaviour()),
            object(5, ClassId::MONO_SCRIPT, script()),
            object(
                6,
                ClassId::GAME_OBJECT,
                game_object("Child", 20_005, &[ptr(0, 7)]),
            ),
            object(7, ClassId::TRANSFORM, transform(6, ptr(0, 3), &[])),
        ],
    );
    let plan = plan_player_scene(dir.path(), "level0", &value_layouts()).unwrap();
    let tags: Vec<_> = plan.objects.iter().map(|o| o.tag.as_str()).collect();
    assert_eq!(tags, ["Enemy", "Untagged"]);
    let [PlanComponent::Script { fields, .. }] = &plan.objects[0].components[..] else {
        panic!("the root has one script: {:?}", plan.objects[0].components)
    };
    assert_eq!(
        *fields,
        json!({
            "big": "18446744073709551615",
            "letter": "233",
            "nan": "NaN",
            "low": "-Infinity",
            "pair": {"values": ["1", "-2"]},
            "shape": Json::Null,
        })
    );
    assert_eq!(
        plan.differences,
        [
            "Root.Example.Spinner.shape: [SerializeReference] values are not built",
            "Child: tag 20005 is not defined",
        ]
    );
}

#[test]
fn carries_the_layer_names_of_the_tag_manager_by_layer_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = ObjectWriter::new(false);
    manager.i32(0).i32(10);
    for layer in 0..10 {
        let name = match layer {
            5 => "UI",
            9 => "Lights",
            _ => "",
        };
        manager.string(name).unwrap();
    }
    manager.i32(0).i32(0);
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::TAG_MANAGER, manager.finish())],
    );
    write_file(dir.path(), "level0", vec![]);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    assert_eq!(plan.layers.len(), 32);
    assert_eq!(plan.layers[9], "Lights");
    assert_eq!(plan.layers[5], "UI");
    assert!(plan.layers[10..].iter().all(String::is_empty));
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["layers"][9], "Lights");
}

#[test]
fn leaves_the_layers_out_of_the_plan_without_a_tag_manager() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &layouts()).unwrap();
    assert!(plan.layers.is_empty());
    assert!(serde_json::to_value(&plan).unwrap().get("layers").is_none());
}
