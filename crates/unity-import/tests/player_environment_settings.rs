//! A player scene's `RenderSettings` object reaches the scene plan's environment beyond its ambient
//! light and reflection cubes: fog, the subtractive shadow colour, reflection bounces and
//! resolution, flares, the sun as a planned light component and the skybox as a planned material
//! asset. What the plan cannot name is null and listed in the differences.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::player_plan::plan_player_scene;
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

#[path = "support/renderer_bytes.rs"]
mod renderer_bytes;
#[path = "support/shader_bytes.rs"]
mod shader_bytes;
use renderer_bytes::{
    Rig, Settings, distinct, filter_bytes, game_object, mesh_renderer_bytes, skinned_bytes,
    transform,
};
use shader_bytes::lit;

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 9] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::LIGHT,
    ClassId::MESH_FILTER,
    ClassId::MESH_RENDERER,
    ClassId::SKINNED_MESH_RENDERER,
    ClassId::RENDER_SETTINGS,
    ClassId::SHADER,
    ClassId::MATERIAL,
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

/// A directional light, enabled, with no cookie or flare.
fn light_bytes() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 0))
        .u8(1)
        .align()
        .i32(1)
        .floats(&[1.0, 0.5, 0.25, 1.0]);
    w.floats(&[2.0, 12.0, 30.0, 20.0]).floats(&[4.0, 4.0]);
    w.i32(0).i32(-1).i32(512).floats(&[0.75, 0.0625, 0.5, 0.25]);
    w.floats(&[0.0; 16]).u8(0).align();
    w.pptr(ptr(0, 0)).u8(0).align();
    w.i32(-1).i32(0).i32(2).i32(0).u8(0).align();
    w.pptr(ptr(0, 0))
        .i32(0)
        .u32(0xff)
        .u32(3)
        .i32(2)
        .i32(0)
        .floats(&[1.0, 1.0])
        .f32(1.5);
    w.f32(5_000.0)
        .u8(1)
        .align()
        .floats(&[0.0; 4])
        .bytes(&[0, 0, 0])
        .align();
    w.i32(0).f32(1.0).u8(0).align();
    w.finish()
}

/// A material with no textures, properties or keywords.
fn material(name: &str, shader: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().pptr(shader);
    w.i32(0).i32(0);
    w.u32(4).u8(1).u8(0).align().i32(-1);
    w.i32(0).i32(0).i32(0);
    w.i32(0).i32(0).i32(0).i32(0);
    w.finish()
}

/// The 27 probe coefficients, `0.5 * i` for slot `i`.
fn probe() -> [f32; 27] {
    std::array::from_fn(|i| 0.5 * f32::from(u8::try_from(i).unwrap()))
}

/// A `RenderSettings` object whose sun and skybox are the given references.
fn render_settings_bytes(sun: PPtr, skybox: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.u8(1).align().floats(&[0.125, 0.25, 0.375, 0.5]).i32(3);
    w.floats(&[0.0625, 10.0, 300.0]);
    w.floats(&[1.0, 0.875, 0.75, 0.625])
        .floats(&[0.5, 0.4375, 0.375, 0.3125]);
    w.floats(&[0.25, 0.125, 0.0625, 0.03125]).f32(1.25).i32(1);
    w.floats(&[0.5, 0.75, 0.875, 1.0]).pptr(skybox);
    w.floats(&[0.5, 1.5, 0.75]).pptr(ptr(0, 0)).pptr(ptr(0, 0));
    w.i32(0).i32(256).i32(3).f32(0.875).pptr(ptr(0, 0));
    w.floats(&probe()).pptr(ptr(0, 0)).pptr(sun);
    w.floats(&[0.0; 4]).u8(0);
    w.finish()
}

/// A scene with the object `Lamp` (component 0 a light, component 1 a mesh renderer drawing the
/// material `Sky`), an object `Body` with a mesh filter and a skinned renderer without materials,
/// the given `RenderSettings` and a second material, `Spare`, nothing draws.
fn write_scene(dir: &Path, settings: Option<Vec<u8>>) {
    let mut objects = vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Lamp", &[ptr(0, 3), ptr(0, 4), ptr(0, 5)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[])),
        object(4, ClassId::LIGHT, light_bytes()),
        object(
            5,
            ClassId::MESH_RENDERER,
            mesh_renderer_bytes(2, &distinct(), &[ptr(1, 10)]),
        ),
        object(
            7,
            ClassId::GAME_OBJECT,
            game_object("Body", &[ptr(0, 8), ptr(0, 12), ptr(0, 13)]),
        ),
        object(8, ClassId::TRANSFORM, transform(7, ptr(0, 0), &[])),
        object(12, ClassId::MESH_FILTER, filter_bytes(7, ptr(0, 0))),
        object(
            13,
            ClassId::SKINNED_MESH_RENDERER,
            skinned_bytes(
                7,
                &Settings {
                    enabled: true,
                    ..distinct()
                },
                ptr(0, 0),
                &[ptr(0, 0)],
                &Rig::default(),
            ),
        ),
    ];
    objects.extend(settings.map(|bytes| object(6, ClassId::RENDER_SETTINGS, bytes)));
    write_file(dir, "level0", objects, &["sharedassets0.assets"]);
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(9, ClassId::SHADER, lit()),
            object(10, ClassId::MATERIAL, material("Sky", ptr(0, 9))),
            object(11, ClassId::MATERIAL, material("Spare", ptr(0, 9))),
        ],
        &[],
    );
}

fn plan(settings: Option<Vec<u8>>) -> (Json, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path(), settings);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    (serde_json::to_value(&plan).unwrap(), plan.differences)
}

fn expected(sun: &Json, skybox: &Json) -> Json {
    json!({
        "ambient_mode": 1, "ambient_sky": [1.0, 0.875, 0.75, 0.625],
        "ambient_equator": [0.5, 0.4375, 0.375, 0.3125],
        "ambient_ground": [0.25, 0.125, 0.0625, 0.03125],
        "ambient_intensity": 1.25, "ambient_probe": probe().map(f64::from),
        "reflection_mode": 0, "reflection_intensity": 0.875,
        "custom_reflection": null, "skybox_reflection": null,
        "reflection_bounces": 3, "reflection_resolution": 256,
        "fog_enabled": true, "fog_color": [0.125, 0.25, 0.375, 0.5], "fog_mode": 3,
        "fog_density": 0.0625, "fog_start": 10.0, "fog_end": 300.0,
        "subtractive_shadow_color": [0.5, 0.75, 0.875, 1.0],
        "halo_strength": 0.5, "flare_strength": 1.5, "flare_fade_speed": 0.75,
        "sun": sun, "skybox_material": skybox,
    })
}

#[test]
fn carries_the_scene_settings_with_the_sun_light_and_the_planned_skybox() {
    let (json, differences) = plan(Some(render_settings_bytes(ptr(0, 4), ptr(1, 10))));
    let sky = json["assets"]
        .as_array()
        .unwrap()
        .iter()
        .position(|asset| asset["type"] == "Material" && asset["name"] == "Sky")
        .unwrap();
    assert_eq!(json["objects"][0]["components"][0]["type"], "Light");
    assert_eq!(json["environment"], expected(&json!([0, 0]), &json!(sky)));
    assert_eq!(differences, Vec::<String>::new());
}

#[test]
fn a_sun_that_is_not_a_planned_light_is_null_and_listed() {
    for sun in [ptr(0, 5), ptr(1, 4), ptr(0, 99)] {
        let (json, differences) = plan(Some(render_settings_bytes(sun, ptr(0, 0))));
        assert_eq!(json["environment"]["sun"], Json::Null, "{sun:?}");
        let listed: Vec<_> = differences
            .iter()
            .filter(|line| line.starts_with("render settings: the sun"))
            .collect();
        assert_eq!(listed.len(), 1, "{sun:?}: {differences:?}");
    }
}

#[test]
fn a_skybox_the_plan_does_not_hold_is_null_and_listed() {
    for skybox in [ptr(1, 11), ptr(1, 99), ptr(0, 99)] {
        let (json, differences) = plan(Some(render_settings_bytes(ptr(0, 0), skybox)));
        assert_eq!(
            json["environment"]["skybox_material"],
            Json::Null,
            "{skybox:?}"
        );
        let listed: Vec<_> = differences
            .iter()
            .filter(|line| line.starts_with("render settings: the skybox material"))
            .collect();
        assert_eq!(listed.len(), 1, "{skybox:?}: {differences:?}");
    }
}

#[test]
fn no_sun_and_no_skybox_are_null_and_list_nothing() {
    let (json, differences) = plan(Some(render_settings_bytes(ptr(0, 0), ptr(0, 0))));
    assert_eq!(json["environment"], expected(&Json::Null, &Json::Null));
    assert_eq!(differences, Vec::<String>::new());
}

#[test]
fn a_scene_without_a_render_settings_object_has_no_environment() {
    let (json, _) = plan(None);
    assert!(json.get("environment").is_none());
}
