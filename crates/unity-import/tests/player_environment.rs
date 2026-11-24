//! A player scene's render settings as the scene plan's environment: the ambient light, the
//! reflection and the cubemaps they name, planned as scene assets with their pixels named in place.

use std::path::Path;

use serde_json::{Value, json};
use worldrunner_unity_import::{plan::ScenePlan, player_plan::plan_player_scene};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 4] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::RENDER_SETTINGS,
    ClassId::CUBEMAP,
];

const PROBE: [f32; 27] = [
    0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0,
    9.5, 10.0, 10.5, 11.0, 11.5, 12.0, 12.5, 13.0,
];

/// The pixels of the cube the level holds and of the one the shared assets hold, each a distinct
/// run so a plan that points at the wrong bytes is seen.
const CUSTOM: [u8; 8] = [10, 11, 12, 13, 14, 15, 16, 17];
const BAKED: [u8; 8] = [90, 91, 92, 93, 94, 95, 96, 97];

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

/// The size, `TextureFormat`, mip levels and byte size of the faces of a cube.
#[derive(Clone, Copy)]
struct Shape {
    side: i32,
    format: i32,
    mips: i32,
    complete: u32,
}

/// 2x2 faces in `TextureFormat` 4 with one mip level.
const RGBA32_2X2: Shape = Shape {
    side: 2,
    format: 4,
    mips: 1,
    complete: 32,
};

/// 8x8 faces in BC6H (`TextureFormat` 24) with the four levels 8x8, 4x4, 2x2 and 1x1, which hold
/// 4 + 1 + 1 + 1 blocks of 16 bytes.
const BC6H_8X8: Shape = Shape {
    side: 8,
    format: 24,
    mips: 4,
    complete: 112,
};

/// A cube of 2x2 faces in `TextureFormat` 4 with one mip level and `images` faces, whose pixels
/// are `pixels`.
fn cube(name: &str, images: i32, pixels: &[u8]) -> Vec<u8> {
    cube_of(name, images, RGBA32_2X2, pixels)
}

/// A cube of the faces `shape` describes and `images` of them, whose pixels are `pixels`.
fn cube_of(name: &str, images: i32, shape: Shape, pixels: &[u8]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().u8(1).align();
    w.i32(shape.side)
        .i32(shape.side)
        .u32(shape.complete)
        .i32(0)
        .i32(shape.format)
        .i32(shape.mips);
    w.u8(1)
        .u8(0)
        .u8(0)
        .align()
        .string("")
        .unwrap()
        .u8(0)
        .align()
        .i32(0);
    w.i32(images)
        .i32(2)
        .i32(2)
        .i32(8)
        .f32(0.5)
        .i32(1)
        .i32(1)
        .i32(1);
    w.i32(0).i32(1).byte_array(&[]).unwrap();
    w.byte_array(pixels).unwrap();
    w.u64(0).u32(0).string("").unwrap().align();
    w.i32(0);
    w.finish()
}

/// The settings of a scene whose ambient light is a trilight, whose reflection is `mode`, and
/// that names the cubes `custom` and `baked`.
fn settings(mode: i32, custom: PPtr, baked: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.u8(0).align().floats(&[0.1, 0.2, 0.3, 0.4]).i32(3);
    w.floats(&[0.01, 10.0, 300.0]);
    w.floats(&[1.0, 0.9, 0.8, 0.7])
        .floats(&[0.6, 0.5, 0.4, 0.3]);
    w.floats(&[0.2, 0.1, 0.05, 0.02]).f32(1.25).i32(1);
    w.floats(&[0.42, 0.48, 0.63, 1.0]).pptr(ptr(0, 0));
    w.floats(&[0.5, 1.5, 0.75]).pptr(ptr(0, 0)).pptr(ptr(0, 0));
    w.i32(mode).i32(128).i32(2).f32(0.9).pptr(custom);
    w.floats(&PROBE).pptr(baked).pptr(ptr(0, 0));
    w.floats(&[0.3, 0.35, 0.4, 1.0]).u8(0);
    w.finish()
}

fn root(dir: &Path, extra: Vec<ObjectData>, externals: &[&str]) {
    let game_object = {
        let mut w = ObjectWriter::new(false);
        w.pptrs(&[ptr(0, 3)])
            .unwrap()
            .u32(0)
            .string("Root")
            .unwrap();
        w.u16(0).u8(1);
        w.finish()
    };
    let transform = {
        let mut w = ObjectWriter::new(false);
        w.pptr(ptr(0, 2)).floats(&[0.0, 0.0, 0.0, 1.0]);
        w.floats(&[0.0; 3]).floats(&[1.0; 3]);
        w.pptrs(&[]).unwrap().pptr(ptr(0, 0));
        w.finish()
    };
    let mut objects = vec![
        object(2, ClassId::GAME_OBJECT, game_object),
        object(3, ClassId::TRANSFORM, transform),
    ];
    objects.extend(extra);
    write_file(dir, "level0", objects, externals);
}

fn write_shared_assets(dir: &Path) {
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(30, ClassId::CUBEMAP, cube("Baked", 6, &BAKED)),
            object(31, ClassId::CUBEMAP, cube("Flat", 1, &BAKED)),
        ],
        &[],
    );
}

/// A scene with the settings `render` and the cubes 20 of the level and 30 and 31 of its shared
/// assets.
fn planned(render: Option<Vec<u8>>) -> (tempfile::TempDir, ScenePlan) {
    let dir = tempfile::tempdir().unwrap();
    let mut extra = vec![object(20, ClassId::CUBEMAP, cube("Custom", 6, &CUSTOM))];
    if let Some(render) = render {
        extra.push(object(21, ClassId::RENDER_SETTINGS, render));
    }
    root(dir.path(), extra, &["sharedassets0.assets"]);
    write_shared_assets(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    (dir, plan)
}

fn as_json(plan: &ScenePlan) -> Value {
    serde_json::to_value(plan).unwrap()
}

/// The bytes the planned asset `index` names.
fn pixels(plan: &Value, index: usize) -> Vec<u8> {
    let data = &plan["assets"][index]["data"];
    let file = data["file"].as_str().unwrap();
    let (offset, size) = (
        data["offset"].as_u64().unwrap(),
        data["size"].as_u64().unwrap(),
    );
    let bytes = std::fs::read(file).unwrap();
    let (start, end) = (
        usize::try_from(offset).unwrap(),
        usize::try_from(offset + size).unwrap(),
    );
    bytes[start..end].to_vec()
}

/// The numbers of a JSON array as the `f32`s they were written from.
fn floats(value: &Value) -> Vec<f32> {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn the_render_settings_of_a_scene_are_its_environment() {
    let (_dir, plan) = planned(Some(settings(1, ptr(0, 20), ptr(1, 30))));
    let plan = as_json(&plan);
    let environment = &plan["environment"];
    assert_eq!(environment["ambient_mode"], 1);
    assert_eq!(floats(&environment["ambient_sky"]), [1.0, 0.9, 0.8, 0.7]);
    assert_eq!(
        floats(&environment["ambient_equator"]),
        [0.6, 0.5, 0.4, 0.3]
    );
    assert_eq!(
        floats(&environment["ambient_ground"]),
        [0.2, 0.1, 0.05, 0.02]
    );
    assert_eq!(environment["ambient_intensity"], 1.25);
    assert_eq!(floats(&environment["ambient_probe"]), PROBE);
    assert_eq!(environment["reflection_mode"], 1);
    assert_eq!(floats(&json!([environment["reflection_intensity"]])), [0.9]);
}

#[test]
fn the_cubes_a_scene_names_are_scene_assets_with_their_pixels_named_in_place() {
    let (_dir, plan) = planned(Some(settings(1, ptr(0, 20), ptr(1, 30))));
    let plan = as_json(&plan);
    let environment = &plan["environment"];
    let custom = usize::try_from(environment["custom_reflection"].as_u64().unwrap()).unwrap();
    let baked = usize::try_from(environment["skybox_reflection"].as_u64().unwrap()).unwrap();
    assert_ne!(custom, baked);
    for (index, name, expected) in [(custom, "Custom", CUSTOM), (baked, "Baked", BAKED)] {
        let asset = &plan["assets"][index];
        assert_eq!(asset["type"], "Cubemap");
        assert_eq!(asset["name"], name);
        assert_eq!((&asset["width"], &asset["height"]), (&json!(2), &json!(2)));
        assert_eq!(pixels(&plan, index), expected, "{name}");
    }
}

/// A scene as the player stores one with a baked sky: the skybox mode, no custom cube and the
/// generated skybox reflection a BC6H cube with a mip chain in the shared assets.
fn planned_bc6h(sky: &[u8]) -> (tempfile::TempDir, ScenePlan) {
    let dir = tempfile::tempdir().unwrap();
    root(
        dir.path(),
        vec![object(
            21,
            ClassId::RENDER_SETTINGS,
            settings(0, ptr(0, 0), ptr(1, 32)),
        )],
        &["sharedassets0.assets"],
    );
    write_file(
        dir.path(),
        "sharedassets0.assets",
        vec![object(
            32,
            ClassId::CUBEMAP,
            cube_of("Sky", 6, BC6H_8X8, sky),
        )],
        &[],
    );
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    (dir, plan)
}

#[test]
fn a_generated_skybox_reflection_in_bc6h_is_planned_with_its_format_and_whole_mip_chain() {
    let sky: Vec<u8> = (0..=u8::MAX).cycle().take(6 * 112).collect();
    let (_dir, plan) = planned_bc6h(&sky);
    let plan = as_json(&plan);
    let environment = &plan["environment"];
    assert_eq!(environment["reflection_mode"], 0);
    assert_eq!(environment["custom_reflection"], Value::Null);
    let index = usize::try_from(environment["skybox_reflection"].as_u64().unwrap()).unwrap();
    let asset = &plan["assets"][index];
    assert_eq!(asset["type"], "Cubemap");
    assert_eq!(asset["name"], "Sky");
    assert_eq!(
        (
            &asset["width"],
            &asset["height"],
            &asset["format"],
            &asset["mip_count"]
        ),
        (&json!(8), &json!(8), &json!(24), &json!(4))
    );
    assert_eq!(pixels(&plan, index), sky);
    assert!(
        plan.get("differences").is_none(),
        "{:?}",
        plan["differences"]
    );
}

#[test]
fn a_cube_both_references_name_is_planned_once() {
    let (_dir, plan) = planned(Some(settings(1, ptr(1, 30), ptr(1, 30))));
    let plan = as_json(&plan);
    let environment = &plan["environment"];
    assert_eq!(
        environment["custom_reflection"],
        environment["skybox_reflection"]
    );
    assert_eq!(plan["assets"].as_array().unwrap().len(), 1);
}

#[test]
fn a_reference_to_no_cube_plans_null_without_a_difference() {
    let (_dir, plan) = planned(Some(settings(0, ptr(0, 0), ptr(0, 0))));
    let plan = as_json(&plan);
    assert_eq!(plan["environment"]["custom_reflection"], Value::Null);
    assert_eq!(plan["environment"]["skybox_reflection"], Value::Null);
    assert_eq!(plan["assets"].as_array().unwrap().len(), 0);
    assert!(
        plan.get("differences").is_none(),
        "{:?}",
        plan["differences"]
    );
}

#[test]
fn a_cube_the_engine_cannot_build_plans_null_and_is_listed_as_a_difference() {
    // The first cube has one image where a cube has six, and the second reference names the
    // render settings object, which is no cube.
    let (_dir, plan) = planned(Some(settings(1, ptr(1, 31), ptr(0, 21))));
    let plan = as_json(&plan);
    assert_eq!(plan["environment"]["custom_reflection"], Value::Null);
    assert_eq!(plan["environment"]["skybox_reflection"], Value::Null);
    let differences = plan["differences"].as_array().unwrap();
    assert_eq!(differences.len(), 2, "{differences:?}");
    assert!(
        differences[0]
            .as_str()
            .unwrap()
            .contains("custom reflection")
    );
    assert!(differences[0].as_str().unwrap().contains("Flat"));
    assert!(
        differences[1]
            .as_str()
            .unwrap()
            .contains("skybox reflection")
    );
}

#[test]
fn a_scene_without_render_settings_has_no_environment() {
    let (_dir, plan) = planned(None);
    assert!(plan.environment.is_none());
    assert!(as_json(&plan).get("environment").is_none());
}
