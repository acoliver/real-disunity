//! Mesh filters, mesh renderers and skinned mesh renderers in player scenes and prefabs: the
//! renderer settings carry into the plan, and a Mesh or Material reference the plans cannot
//! build yet binds null and is listed once.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::{
    plan::PlanComponent,
    player_plan::plan_player_scene,
    player_resources::{ResourceAsset, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 6] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MESH_FILTER,
    ClassId::MESH_RENDERER,
    ClassId::SKINNED_MESH_RENDERER,
    ClassId::RESOURCE_MANAGER,
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
            .map(|path| worldrunner_unity_serialized::External {
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

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components).unwrap().u32(0).string(name).unwrap();
    w.u16(0).u8(1);
    w.finish()
}

fn transform(game_object: i64, father: PPtr, children: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(children).unwrap().pptr(father);
    w.finish()
}

/// The values a renderer's plan carries, set apart from the defaults. The three renderer flags are
/// bytes, 0 or 1, as the serialized form stores them.
struct Settings {
    enabled: bool,
    cast_shadows: u8,
    receive_shadows: u8,
    dynamic_occludee: u8,
    static_shadow_caster: u8,
    motion_vectors: u8,
    light_probe_usage: u8,
    reflection_probe_usage: u8,
    layer_mask: u32,
    priority: i32,
    sorting_layer_id: i32,
    sorting_order: i16,
}

fn distinct() -> Settings {
    Settings {
        enabled: false,
        cast_shadows: 3,
        receive_shadows: 0,
        dynamic_occludee: 0,
        static_shadow_caster: 1,
        motion_vectors: 2,
        light_probe_usage: 2,
        reflection_probe_usage: 3,
        layer_mask: 0xF0F0,
        priority: -4,
        sorting_layer_id: 0,
        sorting_order: 7,
    }
}

fn write_renderer(w: &mut ObjectWriter, owner: i64, s: &Settings, materials: &[PPtr]) {
    w.pptr(ptr(0, owner)).u8(u8::from(s.enabled));
    w.bytes(&[
        s.cast_shadows,
        s.receive_shadows,
        s.dynamic_occludee,
        s.static_shadow_caster,
        s.motion_vectors,
        s.light_probe_usage,
        s.reflection_probe_usage,
        0,
        0,
        0,
        0,
        0,
    ])
    .align();
    w.i16(-1).align().f32(0.0).u32(s.layer_mask).i32(s.priority);
    w.u16(0).u16(0).floats(&[1.0, 1.0, 0.0, 0.0]);
    w.floats(&[1.0, 1.0, 0.0, 0.0]);
    w.pptrs(materials).unwrap();
    w.u16(0)
        .u16(0)
        .pptr(ptr(0, 0))
        .pptr(ptr(0, 0))
        .pptr(ptr(0, 0));
    w.align();
    w.i32(s.sorting_layer_id).i16(0).i16(s.sorting_order).i32(0);
}

fn filter_bytes(owner: i64, mesh: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).pptr(mesh);
    w.finish()
}

fn mesh_renderer_bytes(owner: i64, s: &Settings, materials: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    write_renderer(&mut w, owner, s, materials);
    w.pptr(ptr(0, 0)).pptr(ptr(0, 0));
    w.finish()
}

fn skinned_bytes(owner: i64, s: &Settings, mesh: PPtr, materials: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    write_renderer(&mut w, owner, s, materials);
    w.i32(4).u8(1).u8(0).align().pptr(mesh);
    w.pptrs(&[]).unwrap();
    w.i32(0).pptr(ptr(0, 0));
    w.floats(&[0.0; 6]).u8(0).align();
    w.finish()
}

fn rig_objects() -> Vec<ObjectData> {
    let mesh = ptr(1, 500);
    let materials = [ptr(1, 600), ptr(0, 0), ptr(1, 601)];
    let none = [ptr(0, 0)];
    vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Rock", &[ptr(0, 3), ptr(0, 4), ptr(0, 5)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[ptr(0, 7)])),
        object(4, ClassId::MESH_FILTER, filter_bytes(2, mesh)),
        object(
            5,
            ClassId::MESH_RENDERER,
            mesh_renderer_bytes(2, &distinct(), &materials),
        ),
        object(
            6,
            ClassId::GAME_OBJECT,
            game_object("Body", &[ptr(0, 7), ptr(0, 8), ptr(0, 9)]),
        ),
        object(7, ClassId::TRANSFORM, transform(6, ptr(0, 3), &[])),
        object(8, ClassId::MESH_FILTER, filter_bytes(6, ptr(0, 0))),
        object(
            9,
            ClassId::SKINNED_MESH_RENDERER,
            skinned_bytes(
                6,
                &Settings {
                    enabled: true,
                    ..distinct()
                },
                mesh,
                &none,
            ),
        ),
    ]
}

fn assert_rig(components: &[Vec<PlanComponent>], differences: &[String], owner: &str) {
    let [rock, body] = components else {
        panic!("two objects: {components:?}")
    };
    let [PlanComponent::MeshFilter {}, mesh_renderer] = &rock[..] else {
        panic!("a filter and a renderer on the rock: {rock:?}")
    };
    let expected = json!({
        "type": "MeshRenderer", "enabled": false, "receive_shadows": false,
        "dynamic_occludee": false, "static_shadow_caster": true, "shadow_casting_mode": 3,
        "motion_vector_mode": 2, "light_probe_usage": 2, "reflection_probe_usage": 3,
        "rendering_layer_mask": 0xF0F0, "renderer_priority": -4, "sorting_layer_id": 0,
        "sorting_order": 7, "material_count": 3
    });
    assert_eq!(serde_json::to_value(mesh_renderer).unwrap(), expected);
    let [PlanComponent::MeshFilter {}, skinned] = &body[..] else {
        panic!("a filter and a skinned renderer on the body: {body:?}")
    };
    let mut skin = expected.clone();
    skin["type"] = json!("SkinnedMeshRenderer");
    skin["enabled"] = json!(true);
    skin["material_count"] = json!(1);
    skin["quality"] = json!(4);
    skin["update_when_offscreen"] = json!(true);
    assert_eq!(serde_json::to_value(skinned).unwrap(), skin);
    let mut listed = differences.to_vec();
    listed.sort();
    let name = |what: &str, who: &str| format!("{owner}{who}: {what}");
    let mut wanted = vec![
        name(
            "MeshFilter mesh references a Mesh asset, which plans cannot build yet",
            "Rock",
        ),
        name(
            "MeshRenderer materials reference Material assets, which plans cannot build yet",
            "Rock",
        ),
        name(
            "SkinnedMeshRenderer mesh references a Mesh asset, which plans cannot build yet",
            "Body",
        ),
    ];
    wanted.sort();
    assert_eq!(listed, wanted);
}

#[test]
fn a_scene_plans_mesh_filters_and_renderers_with_their_settings() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "level0",
        rig_objects(),
        &["sharedassets0.assets"],
    );
    let layouts = ScriptLayouts::new(Vec::new());
    let plan = plan_player_scene(dir.path(), "level0", &layouts).unwrap();
    let components: Vec<_> = plan.objects.into_iter().map(|o| o.components).collect();
    assert_rig(&components, &plan.differences, "");
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(1).string("prefabs/rock").unwrap().pptr(ptr(1, 2));
    w.i32(0);
    w.finish()
}

#[test]
fn a_prefab_plans_mesh_filters_and_renderers_with_their_settings() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    write_file(
        dir.path(),
        "resources.assets",
        rig_objects(),
        &["sharedassets0.assets"],
    );
    let plan = plan_player_resources(
        dir.path(),
        &ScriptLayouts::new(Vec::new()),
        &dir.path().join("out"),
    )
    .unwrap();
    let ResourceAsset::Prefab { plan: file, .. } = &plan.assets[0] else {
        panic!("the rock is a prefab: {:?}", plan.assets[0])
    };
    let bytes = std::fs::read(dir.path().join("out").join(file)).unwrap();
    let scene: Json = serde_json::from_slice(&bytes).unwrap();
    let objects = scene["objects"].as_array().unwrap();
    assert_eq!(objects[0]["components"][1]["type"], "MeshRenderer");
    assert_eq!(objects[0]["components"][1]["sorting_order"], 7);
    assert_eq!(objects[1]["components"][1]["type"], "SkinnedMeshRenderer");
    assert_eq!(objects[1]["components"][1]["quality"], 4);
    let differences = &plan.differences;
    let rock: Vec<_> = differences
        .iter()
        .filter(|d| d.starts_with("prefab Rock:") && d.contains("Mesh"))
        .collect();
    assert_eq!(rock.len(), 3, "{differences:?}");
}

#[test]
fn a_null_mesh_and_null_materials_list_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let objects = vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Empty", &[ptr(0, 3), ptr(0, 4), ptr(0, 5)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[])),
        object(4, ClassId::MESH_FILTER, filter_bytes(2, ptr(0, 0))),
        object(
            5,
            ClassId::MESH_RENDERER,
            mesh_renderer_bytes(2, &distinct(), &[ptr(0, 0), ptr(0, 0)]),
        ),
    ];
    write_file(dir.path(), "level0", objects, &[]);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    assert_eq!(plan.differences, Vec::<String>::new());
    let json = serde_json::to_value(&plan.objects[0].components[1]).unwrap();
    assert_eq!(json["material_count"], 2);
}

#[test]
fn modes_the_engine_does_not_know_fall_back_and_are_listed() {
    let dir = tempfile::tempdir().unwrap();
    let odd = Settings {
        cast_shadows: 9,
        motion_vectors: 9,
        light_probe_usage: 9,
        reflection_probe_usage: 9,
        ..distinct()
    };
    let objects = vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Odd", &[ptr(0, 3), ptr(0, 5)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[])),
        object(5, ClassId::MESH_RENDERER, mesh_renderer_bytes(2, &odd, &[])),
    ];
    write_file(dir.path(), "level0", objects, &[]);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    let json = serde_json::to_value(&plan.objects[0].components[0]).unwrap();
    assert_eq!(
        [
            &json["shadow_casting_mode"],
            &json["motion_vector_mode"],
            &json["light_probe_usage"],
            &json["reflection_probe_usage"]
        ],
        [&json!(1), &json!(1), &json!(1), &json!(1)]
    );
    assert_eq!(plan.differences.len(), 4, "{:?}", plan.differences);
    assert!(
        plan.differences
            .iter()
            .all(|d| d.starts_with("Odd: MeshRenderer "))
    );
}
