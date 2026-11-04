//! Compute shaders a player's script fields reference, in resource plans and in scene plans.

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

#[path = "support/compute_bytes.rs"]
mod compute_bytes;
use compute_bytes::compute_shader;

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 7] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::COMPUTE_SHADER,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::SHADER,
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

fn behaviour(owner: i64, script: PPtr, references: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).u8(1).align().pptr(script);
    w.string("").unwrap();
    for &reference in references {
        w.pptr(reference);
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
                ty: FieldType::Object("UnityEngine.ComputeShader".to_owned()),
            })
            .collect(),
    }])
}

fn broken() -> Vec<u8> {
    let bytes = compute_shader("Broken", &[("Main", [4, 4, 1])]);
    bytes[..bytes.len() / 2].to_vec()
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(2);
    for (path, target) in [("compute/read", 2), ("config/main", 4)] {
        w.string(path).unwrap().pptr(ptr(1, target));
    }
    w.i32(0);
    w.finish()
}

/// A player whose container lists a compute shader and a script asset that references it, a
/// compute shader the container does not list, one that does not decode, and the first again.
fn planned_resources() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    let references = [ptr(0, 2), ptr(0, 5), ptr(0, 3), ptr(0, 2)];
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(
                2,
                ClassId::COMPUTE_SHADER,
                compute_shader("TerrainReadBackShader", &[("TerrainReadBackCS", [8, 8, 1])]),
            ),
            object(3, ClassId::COMPUTE_SHADER, broken()),
            object(
                4,
                ClassId::MONO_BEHAVIOUR,
                behaviour(0, ptr(0, 6), &references),
            ),
            object(
                5,
                ClassId::COMPUTE_SHADER,
                compute_shader(
                    "Histogram",
                    &[("KGather", [16, 16, 1]), ("KClear", [0, 0, 0])],
                ),
            ),
            object(6, ClassId::MONO_SCRIPT, script()),
        ],
        &[],
    );
    let plan = plan_player_resources(
        dir.path(),
        &layouts(&["read", "hidden", "broken", "again"]),
        &dir.path().join("out"),
    )
    .unwrap();
    (dir, plan)
}

#[test]
fn a_planned_compute_shader_carries_its_file_object_and_kernels() {
    let (_dir, plan) = planned_resources();
    let planned = serde_json::to_value(&plan.assets[0]).unwrap();
    assert_eq!(
        planned,
        json!({
            "type": "ComputeShader",
            "name": "TerrainReadBackShader",
            "file": "resources.assets",
            "path_id": 2,
            "kernels": [{"name": "TerrainReadBackCS", "thread_group_size": [8, 8, 1]}],
        })
    );
}

#[test]
fn a_kernel_without_a_stated_size_plans_null() {
    let (_dir, plan) = planned_resources();
    let planned = serde_json::to_value(&plan.assets[2]).unwrap();
    assert_eq!(planned["name"], "Histogram");
    assert_eq!(
        planned["kernels"],
        json!([
            {"name": "KGather", "thread_group_size": [16, 16, 1]},
            {"name": "KClear", "thread_group_size": null},
        ])
    );
}

#[test]
fn the_container_entry_names_the_class() {
    let (_dir, plan) = planned_resources();
    let entry = plan
        .entries
        .iter()
        .find(|entry| entry.path == "compute/read")
        .unwrap();
    assert_eq!(
        (entry.class.as_str(), entry.asset),
        ("ComputeShader", Some(0))
    );
}

#[test]
fn script_fields_bind_to_the_planned_compute_shaders_and_a_shared_one_to_one_asset() {
    let (dir, plan) = planned_resources();
    let ResourceAsset::Script { fields, .. } = &plan.assets[1] else {
        panic!("asset 1 is the script: {:?}", plan.assets[1])
    };
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("out").join(fields)).unwrap())
            .unwrap();
    assert_eq!(
        written,
        json!({
            "read": {"$asset": 0},
            "hidden": {"$asset": 2},
            "broken": null,
            "again": {"$asset": 0},
        })
    );
}

#[test]
fn a_compute_shader_that_does_not_decode_is_an_error_entry_and_the_rest_still_plans() {
    let (_dir, plan) = planned_resources();
    let Some(ResourceAsset::UnplannedComputeShader { error }) = plan.assets.get(3) else {
        panic!("asset 3 is the broken shader: {:?}", plan.assets)
    };
    assert!(error.contains("resources.assets"), "{error}");
    assert!(
        plan.differences.iter().any(|line| line
            .starts_with("resource .broken: references compute shader object 3")
            && line.contains("did not plan")),
        "{:?}",
        plan.differences
    );
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
                behaviour(2, ptr(0, 5), &references),
            ),
            object(5, ClassId::MONO_SCRIPT, script()),
        ],
        &["sharedassets0.assets"],
    );
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(
                10,
                ClassId::COMPUTE_SHADER,
                compute_shader("Scene/Read", &[("Main", [8, 4, 1])]),
            ),
            object(11, ClassId::COMPUTE_SHADER, broken()),
            object(12, ClassId::SHADER, Vec::new()),
        ],
        &[],
    );
}

#[test]
fn a_scene_script_field_plans_its_compute_shader_as_a_scene_asset_shared_by_both_references() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let layouts = layouts(&["read", "same", "broken", "shader"]);
    let plan = plan_player_scene(dir.path(), "level0", &layouts).unwrap();
    let [
        PlanAsset::ComputeShader {
            name,
            file,
            path_id,
            kernels,
        },
    ] = &plan.assets[..]
    else {
        panic!("the scene holds the one compute shader: {:?}", plan.assets)
    };
    assert_eq!(
        (name.as_str(), file.as_str(), *path_id),
        ("Scene/Read", "sharedassets0.assets", 10)
    );
    assert_eq!(
        serde_json::to_value(kernels).unwrap(),
        json!([{"name": "Main", "thread_group_size": [8, 4, 1]}])
    );
    let [PlanComponent::Script { fields, .. }] = &plan.objects[0].components[..] else {
        panic!("the root has one script: {:?}", plan.objects[0].components)
    };
    assert_eq!(fields["read"], json!({"$asset": 0}),);
    assert_eq!(fields["same"], json!({"$asset": 0}));
    assert_eq!(fields["broken"], json!(null));
    assert!(
        plan.differences
            .iter()
            .any(|line| line.contains("broken") && line.contains("did not plan")),
        "{:?}",
        plan.differences
    );
}
