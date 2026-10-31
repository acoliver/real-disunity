//! Shaders a player's script fields reference, in resource plans and in scene plans.

use std::path::Path;

use serde_json::json;
use worldrunner_unity_import::{
    plan::{PlanAsset, PlanComponent},
    player_plan::plan_player_scene,
    player_resources::{ResourceAsset, ResourcesPlan, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

#[path = "support/shader_bytes.rs"]
mod shader_bytes;
use shader_bytes::{lit, shader};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 6] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::SHADER,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
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

fn broken() -> Vec<u8> {
    let bytes = lit();
    bytes[..bytes.len() / 2].to_vec()
}

fn script() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Config").unwrap().i32(0).bytes(&[0; 16]);
    w.string("Config")
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

/// A behaviour of the script at `script` on `owner`, named `name`, holding the shader
/// references `shaders` in the order of the layout's fields.
fn behaviour(owner: i64, script: PPtr, name: &str, shaders: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).u8(1).align().pptr(script);
    w.string(name).unwrap();
    for &shader in shaders {
        w.pptr(shader);
    }
    w.finish()
}

fn layouts(fields: &[&str]) -> ScriptLayouts {
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Config".to_owned(),
        value_type: false,
        fields: fields
            .iter()
            .map(|name| ScriptField {
                name: (*name).to_owned(),
                ty: FieldType::Object("UnityEngine.Shader".to_owned()),
            })
            .collect(),
    }])
}

const RESOURCE_FIELDS: [&str; 5] = ["lit", "hidden", "broken", "late", "again"];

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    let entries = [
        ("shaders/lit", 2),
        ("shaders/broken", 3),
        ("shaders/odd", 8),
        ("config/main", 4),
    ];
    w.i32(4);
    for (path, target) in entries {
        w.string(path).unwrap().pptr(ptr(1, target));
    }
    w.i32(0);
    w.finish()
}

/// A player whose container lists a shader, a shader that does not decode, a shader with an
/// unknown queue and a script asset that references the first, two the container does not
/// list, the broken one and the first again.
fn planned_resources() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    let references = [ptr(0, 2), ptr(0, 5), ptr(0, 3), ptr(0, 7), ptr(0, 2)];
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(2, ClassId::SHADER, lit()),
            object(3, ClassId::SHADER, broken()),
            object(
                4,
                ClassId::MONO_BEHAVIOUR,
                behaviour(0, ptr(0, 6), "main", &references),
            ),
            object(5, ClassId::SHADER, shader("Test/Hidden", None, &[1, 4])),
            object(6, ClassId::MONO_SCRIPT, script()),
            object(
                7,
                ClassId::SHADER,
                shader("Test/Late", Some(("Queue", "Geometry+200")), &[3, 1]),
            ),
            object(
                8,
                ClassId::SHADER,
                shader("Test/Odd", Some(("QUEUE", "Sideways")), &[1]),
            ),
        ],
        &[],
    );
    let plan = plan_player_resources(
        dir.path(),
        &layouts(&RESOURCE_FIELDS),
        &dir.path().join("out"),
    )
    .unwrap();
    (dir, plan)
}

fn shader_asset(plan: &ResourcesPlan, index: usize) -> (&str, i32, usize, usize) {
    let ResourceAsset::Shader {
        name,
        render_queue,
        pass_count,
        subshader_count,
    } = &plan.assets[index]
    else {
        panic!("asset {index} is a shader: {:?}", plan.assets[index])
    };
    (name, *render_queue, *pass_count, *subshader_count)
}

#[test]
fn plans_container_shaders_with_their_name_queue_and_counts() {
    let (_dir, plan) = planned_resources();
    let entries: Vec<_> = plan
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.class.as_str(), e.asset))
        .collect();
    assert_eq!(
        entries,
        [
            ("shaders/lit", "Shader", Some(0)),
            ("shaders/broken", "Shader", Some(1)),
            ("shaders/odd", "Shader", Some(2)),
            ("config/main", "MonoBehaviour", Some(3)),
        ]
    );
    assert_eq!(shader_asset(&plan, 0), ("Test/Lit", 3000, 2, 1));
}

#[test]
fn plans_a_referenced_shader_the_container_does_not_list_with_the_default_queue() {
    let (_dir, plan) = planned_resources();
    assert_eq!(shader_asset(&plan, 4), ("Test/Hidden", 2000, 1, 2));
    assert_eq!(shader_asset(&plan, 5), ("Test/Late", 2200, 3, 2));
}

#[test]
fn binds_references_to_the_planned_shaders_and_a_shared_one_to_one_asset() {
    let (dir, plan) = planned_resources();
    let ResourceAsset::Script { fields, .. } = &plan.assets[3] else {
        panic!("asset 3 is the script: {:?}", plan.assets[3])
    };
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("out").join(fields)).unwrap())
            .unwrap();
    assert_eq!(
        written,
        json!({
            "lit": {"$asset": 0},
            "hidden": {"$asset": 4},
            "broken": null,
            "late": {"$asset": 5},
            "again": {"$asset": 0},
        })
    );
}

#[test]
fn a_shader_that_does_not_decode_is_an_error_entry_and_the_rest_still_plans() {
    let (_dir, plan) = planned_resources();
    let ResourceAsset::UnplannedShader { error } = &plan.assets[1] else {
        panic!("asset 1 is the broken shader: {:?}", plan.assets[1])
    };
    assert!(error.contains("resources.assets"), "{error}");
    assert!(
        plan.differences.iter().any(|line| line
            .starts_with("resource main.broken: references shader object 3")
            && line.contains("did not plan")),
        "{:?}",
        plan.differences
    );
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("shader ") && line.contains("did not plan")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn an_unknown_queue_name_is_an_error_entry_naming_it() {
    let (_dir, plan) = planned_resources();
    let ResourceAsset::UnplannedShader { error } = &plan.assets[2] else {
        panic!("asset 2 is the odd shader: {:?}", plan.assets[2])
    };
    assert!(error.contains("Sideways"), "{error}");
}

fn write_scene(dir: &Path) {
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
    let references = [ptr(1, 10), ptr(1, 10), ptr(1, 11), ptr(1, 12)];
    write_file(
        dir,
        "level0",
        vec![
            object(2, ClassId::GAME_OBJECT, game_object),
            object(3, ClassId::TRANSFORM, transform),
            object(
                4,
                ClassId::MONO_BEHAVIOUR,
                behaviour(2, ptr(0, 5), "", &references),
            ),
            object(5, ClassId::MONO_SCRIPT, script()),
        ],
        &["sharedassets0.assets"],
    );
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(10, ClassId::SHADER, lit()),
            object(11, ClassId::SHADER, broken()),
            object(12, ClassId::MONO_SCRIPT, script()),
        ],
        &[],
    );
}

#[test]
fn a_scene_script_field_plans_its_shader_as_a_scene_asset_shared_by_both_references() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let layouts = layouts(&["shader", "same", "broken", "script"]);
    let plan = plan_player_scene(dir.path(), "level0", &layouts).unwrap();
    let [
        PlanAsset::Shader {
            name,
            render_queue,
            pass_count,
            subshader_count,
        },
    ] = &plan.assets[..]
    else {
        panic!("the scene holds the one shader: {:?}", plan.assets)
    };
    assert_eq!(
        (name.as_str(), *render_queue, *pass_count, *subshader_count),
        ("Test/Lit", 3000, 2, 1)
    );
    let [PlanComponent::Script { fields, .. }] = &plan.objects[0].components[..] else {
        panic!("the root has one script: {:?}", plan.objects[0].components)
    };
    assert_eq!(
        *fields,
        json!({"shader": {"$asset": 0}, "same": {"$asset": 0}, "broken": null, "script": null})
    );
    assert!(
        plan.differences
            .iter()
            .any(|line| line.contains("broken") && line.contains("did not plan")),
        "{:?}",
        plan.differences
    );
    assert!(
        plan.differences.iter().any(|line| line.contains(".script:")
            && line.contains("object 12 of file 1, which the plan does not hold")),
        "a reference to another class stays unplanned: {:?}",
        plan.differences
    );
}

/// The queue the planner reads from the first sub-shader's queue tag value `text`.
fn planned_queue(text: &str) -> Result<i32, String> {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = ObjectWriter::new(false);
    manager.i32(1).string("s").unwrap().pptr(ptr(1, 2)).i32(0);
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, manager.finish())],
        &["resources.assets"],
    );
    let declared = shader("Test/Queue", Some(("QUEUE", text)), &[1]);
    write_file(
        dir.path(),
        "resources.assets",
        vec![object(2, ClassId::SHADER, declared)],
        &[],
    );
    let plan = plan_player_resources(
        dir.path(),
        &ScriptLayouts::new(Vec::new()),
        &dir.path().join("out"),
    )
    .unwrap();
    match &plan.assets[0] {
        ResourceAsset::Shader { render_queue, .. } => Ok(*render_queue),
        ResourceAsset::UnplannedShader { error } => Err(error.clone()),
        other => panic!("the one asset is a shader: {other:?}"),
    }
}

#[test]
fn reads_queue_names_offsets_and_numbers() {
    let cases = [
        ("Background", 1000),
        ("Geometry", 2000),
        ("AlphaTest", 2450),
        ("Transparent", 3000),
        ("Overlay", 4000),
        ("transparent-50", 2950),
        ("Geometry+200", 2200),
        ("Transparent+0", 3000),
        ("2501", 2501),
    ];
    for (text, expected) in cases {
        assert_eq!(planned_queue(text), Ok(expected), "{text}");
    }
    for text in ["Geometry+x", "Sideways+1", ""] {
        let error = planned_queue(text).unwrap_err();
        assert!(error.contains("cannot read"), "{text}: {error}");
    }
}
