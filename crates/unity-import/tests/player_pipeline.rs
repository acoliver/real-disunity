//! The render pipeline assets a player's `GraphicsSettings` and `QualitySettings` name are
//! planned as script assets, with their fields and references to each other.

use std::path::Path;

#[path = "support/shader_bytes.rs"]
mod shader_bytes;

use serde_json::json;
use worldrunner_unity_import::player_resources::{
    ResourceAsset, ResourcesPlan, plan_player_resources,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

const NONE: PPtr = PPtr {
    file_id: 0,
    path_id: 0,
};

/// A reference to an object of the same file.
const fn local(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

/// A reference into `globalgamemanagers.assets`, which is file 1 of `globalgamemanagers`.
const fn assets(path_id: i64) -> PPtr {
    PPtr {
        file_id: 1,
        path_id,
    }
}

const TYPES: [ClassId; 6] = [
    ClassId::GRAPHICS_SETTINGS,
    ClassId::QUALITY_SETTINGS,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::LIGHT,
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

/// A `GraphicsSettings` in the layout of the Old World player, naming the two assets.
fn graphics_settings(pipeline: PPtr, global_settings: PPtr) -> Vec<u8> {
    graphics_settings_with(pipeline, global_settings, &[])
}

/// A `GraphicsSettings` naming the two assets and the always included `shaders`.
fn graphics_settings_with(pipeline: PPtr, global_settings: PPtr, shaders: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    for mode in 0..7 {
        w.i32(mode).pptr(NONE);
    }
    w.i32(2).pptrs(shaders).unwrap().pptrs(&[]).unwrap();
    w.i32(-1).pptr(NONE).pptr(pipeline);
    w.i32(0).floats(&[0.0, 0.0, 1.0]).i32(1).i32(1);
    w.bytes(&[0; 132]);
    w.pptr(global_settings);
    w.bytes(&[0; 12]);
    w.finish()
}

fn quality_settings(pipelines: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(0).i32(i32::try_from(pipelines.len()).unwrap());
    for pipeline in pipelines {
        w.string("Level").unwrap().bytes(&[0; 144]);
        w.pptr(*pipeline).bytes(&[0; 36]);
    }
    w.bytes(&[0; 8]);
    w.finish()
}

/// A script of the `Example.Game` assembly named `class`.
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

/// A script asset named `name` of the script at `script` in the same file, with a level and
/// a reference to another asset.
fn asset(script: i64, name: &str, level: i32, other: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(NONE)
        .u8(1)
        .align()
        .pptr(PPtr {
            file_id: 0,
            path_id: script,
        })
        .string(name)
        .unwrap()
        .i32(level)
        .pptr(other);
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    let fields = || {
        vec![
            field("level", FieldType::Primitive(Primitive::I32)),
            field("other", FieldType::Object("UnityEngine.Object".to_owned())),
        ]
    };
    ScriptLayouts::new(
        ["Pipeline", "Settings"]
            .map(|name| ScriptType {
                key: format!("Example.Game:Example.{name}"),
                value_type: false,
                fields: fields(),
            })
            .to_vec(),
    )
}

/// Plans a player whose managers name `pipeline`, `global_settings` and one level per entry
/// of `levels`. `globalgamemanagers.assets` holds a pipeline asset (10), a settings asset
/// (11), a second pipeline asset (12), their scripts (13 and 14) and a light (15).
fn planned(
    pipeline: PPtr,
    global_settings: PPtr,
    levels: &[PPtr],
) -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![
            object(
                1,
                ClassId::GRAPHICS_SETTINGS,
                graphics_settings(pipeline, global_settings),
            ),
            object(2, ClassId::QUALITY_SETTINGS, quality_settings(levels)),
        ],
        &["globalgamemanagers.assets"],
    );
    write_file(
        dir.path(),
        "globalgamemanagers.assets",
        vec![
            object(10, ClassId::MONO_BEHAVIOUR, asset(13, "High", 3, local(11))),
            object(
                11,
                ClassId::MONO_BEHAVIOUR,
                asset(14, "Global", 4, local(10)),
            ),
            object(12, ClassId::MONO_BEHAVIOUR, asset(13, "Low", 1, NONE)),
            object(13, ClassId::MONO_SCRIPT, script("Pipeline")),
            object(14, ClassId::MONO_SCRIPT, script("Settings")),
            object(15, ClassId::LIGHT, Vec::new()),
        ],
        &[],
    );
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    (dir, plan)
}

fn script_asset(plan: &ResourcesPlan, index: usize) -> (&str, &str, &str) {
    let ResourceAsset::Script {
        class,
        name,
        fields,
        ..
    } = &plan.assets[index]
    else {
        panic!(
            "asset {index} is not a script asset: {:?}",
            plan.assets[index]
        )
    };
    (class, name, fields)
}

fn fields_of(dir: &tempfile::TempDir, plan: &ResourcesPlan, index: usize) -> serde_json::Value {
    let (_, _, fields) = script_asset(plan, index);
    let bytes = std::fs::read(dir.path().join("out").join(fields)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn plans_the_pipeline_and_the_global_settings_with_their_fields() {
    let (dir, plan) = planned(assets(10), assets(11), &[]);
    assert_eq!(plan.graphics.render_pipeline, Some(0));
    assert_eq!(plan.graphics.global_settings, Some(1));
    assert_eq!(plan.assets.len(), 2);
    assert_eq!(script_asset(&plan, 0).0, "Example.Pipeline");
    assert_eq!(script_asset(&plan, 0).1, "High");
    assert_eq!(script_asset(&plan, 1).1, "Global");
    // Each asset's reference to the other binds to the planned asset.
    assert_eq!(
        fields_of(&dir, &plan, 0),
        json!({"level": "3", "other": {"$asset": 1}})
    );
    assert_eq!(
        fields_of(&dir, &plan, 1),
        json!({"level": "4", "other": {"$asset": 0}})
    );
    assert_eq!(plan.differences, Vec::<String>::new());
}

#[test]
fn a_player_without_a_pipeline_plans_none() {
    let (_dir, plan) = planned(NONE, NONE, &[NONE, NONE]);
    assert_eq!(plan.graphics.render_pipeline, None);
    assert_eq!(plan.graphics.global_settings, None);
    assert_eq!(plan.graphics.quality_pipelines, [None, None]);
    assert!(plan.assets.is_empty());
    assert!(plan.differences.is_empty());
}

#[test]
fn each_quality_level_names_its_own_asset_and_shared_assets_plan_once() {
    let (_dir, plan) = planned(assets(10), NONE, &[NONE, assets(12), assets(10)]);
    assert_eq!(plan.graphics.render_pipeline, Some(0));
    assert_eq!(plan.graphics.quality_pipelines, [None, Some(1), Some(0)]);
    assert_eq!(script_asset(&plan, 1).1, "Low");
}

#[test]
fn an_asset_of_a_class_the_engine_does_not_build_is_a_difference_not_a_pipeline() {
    let (_dir, plan) = planned(assets(15), assets(11), &[]);
    assert_eq!(plan.graphics.render_pipeline, None);
    assert_eq!(plan.graphics.global_settings, Some(0));
    assert!(
        plan.differences
            .iter()
            .any(|line| line == "the render pipeline asset, object 15 of file 1, is a Light, which the engine does not build"),
        "{:?}",
        plan.differences
    );
}

/// The always included shaders plan as shader assets in settings order; a null reference is
/// skipped and an object the engine does not build is a difference.
#[test]
fn always_included_shaders_plan_as_shaders_in_settings_order() {
    let dir = tempfile::tempdir().unwrap();
    let shaders = [assets(17), NONE, assets(15), assets(16)];
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(
            1,
            ClassId::GRAPHICS_SETTINGS,
            graphics_settings_with(NONE, NONE, &shaders),
        )],
        &["globalgamemanagers.assets"],
    );
    write_file(
        dir.path(),
        "globalgamemanagers.assets",
        vec![
            object(15, ClassId::LIGHT, Vec::new()),
            object(16, ClassId::SHADER, shader_bytes::lit()),
            object(
                17,
                ClassId::SHADER,
                shader_bytes::shader("Mohawk/Packed", None, &[2]),
            ),
        ],
        &[],
    );
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    let names: Vec<String> = plan
        .graphics
        .always_included_shaders
        .iter()
        .map(|&index| match &plan.assets[index] {
            ResourceAsset::Shader { name, .. } => name.clone(),
            other => panic!("not a shader: {other:?}"),
        })
        .collect();
    assert_eq!(names, ["Mohawk/Packed", "Test/Lit"]);
    assert!(
        plan.differences
            .iter()
            .any(|line| line.contains("always included shader") && line.contains("Light")),
        "{:?}",
        plan.differences
    );
}
