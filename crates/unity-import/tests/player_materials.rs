//! Materials that script fields, renderers and the resource container of a player reference,
//! in resource plans and in scene plans.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::{
    plan::PlanAsset,
    player_plan::plan_player_scene,
    player_resources::{ResourceAsset, ResourcesPlan, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

#[path = "support/renderer_bytes.rs"]
mod renderer_bytes;
#[path = "support/shader_bytes.rs"]
mod shader_bytes;
use renderer_bytes::{
    distinct, filter_bytes, game_object, mesh_renderer_bytes, skinned_bytes, transform,
};
use shader_bytes::lit;

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 11] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::SHADER,
    ClassId::MATERIAL,
    ClassId::TEXTURE_2D,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MESH_FILTER,
    ClassId::MESH_RENDERER,
    ClassId::SKINNED_MESH_RENDERER,
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

/// A material named `name` of `shader`, queue 3100, keywords `_A` and `_B`, the tag
/// `RenderType=Opaque`, the disabled pass `ShadowCaster`, the texture slots `textures` (scale
/// (2, 3), offset (0.5, 0.25)), the int `_Mode` 2, the float `_Cutoff` 0.5 and the color
/// `_Color`.
fn material(name: &str, shader: PPtr, textures: &[(&str, PPtr)]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().pptr(shader);
    w.i32(2).string("_A").unwrap().string("_B").unwrap().i32(0);
    w.u32(4).u8(1).u8(0).align().i32(3100);
    w.i32(1)
        .string("RenderType")
        .unwrap()
        .string("Opaque")
        .unwrap();
    w.i32(1).string("ShadowCaster").unwrap();
    w.i32(i32::try_from(textures.len()).unwrap());
    for &(slot, texture) in textures {
        w.string(slot).unwrap().pptr(texture);
        w.floats(&[2.0, 3.0]).floats(&[0.5, 0.25]);
    }
    w.i32(1).string("_Mode").unwrap().i32(2);
    w.i32(1).string("_Cutoff").unwrap().f32(0.5);
    w.i32(1)
        .string("_Color")
        .unwrap()
        .floats(&[1.0, 0.5, 0.25, 1.0]);
    w.i32(0);
    w.finish()
}

/// A 2x2 RGBA32 texture of one mip level with its pixels inline.
fn texture() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Albedo").unwrap().u8(0).align();
    w.i32(2).i32(2).u32(16).i32(0).i32(4).i32(1);
    w.u8(1).u8(0).u8(0).align();
    w.string("").unwrap().u8(0).align().i32(0).align();
    w.i32(1).i32(2);
    w.i32(1).i32(4).f32(0.0).i32(0).i32(1).i32(2);
    w.i32(6).i32(1).byte_array(&[]).unwrap();
    w.byte_array(&[1; 16])
        .unwrap()
        .u64(0)
        .u32(0)
        .string("")
        .unwrap();
    w.finish()
}

fn broken() -> Vec<u8> {
    let bytes = material("Broken", ptr(0, 2), &[]);
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

fn behaviour(materials: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 0)).u8(1).align().pptr(ptr(0, 6));
    w.string("main").unwrap();
    for &material in materials {
        w.pptr(material);
    }
    w.finish()
}

fn layouts() -> ScriptLayouts {
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Config".to_owned(),
        value_type: false,
        fields: ["a", "b", "broken"]
            .iter()
            .map(|name| ScriptField {
                name: (*name).to_owned(),
                ty: FieldType::Object("UnityEngine.Material".to_owned()),
            })
            .collect(),
    }])
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    let entries = [
        ("materials/lit", 10),
        ("materials/broken", 11),
        ("config/main", 4),
        ("prefabs/rock", 20),
    ];
    w.i32(4);
    for (path, target) in entries {
        w.string(path).unwrap().pptr(ptr(1, target));
    }
    w.i32(0);
    w.finish()
}

/// A player whose container lists a material, a material that does not decode, a script asset
/// that references both and a prefab whose renderers reference the first, one the container
/// does not list (without a shader, with a texture the player does not hold) and the broken one.
fn planned_resources() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    let slots = [("_MainTex", ptr(0, 3)), ("_Bump", ptr(0, 0))];
    let renderer_materials = [ptr(0, 10), ptr(0, 12), ptr(0, 0)];
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(2, ClassId::SHADER, lit()),
            object(3, ClassId::TEXTURE_2D, texture()),
            object(
                4,
                ClassId::MONO_BEHAVIOUR,
                behaviour(&[ptr(0, 10), ptr(0, 10), ptr(0, 11)]),
            ),
            object(6, ClassId::MONO_SCRIPT, script()),
            object(10, ClassId::MATERIAL, material("Lit", ptr(0, 2), &slots)),
            object(11, ClassId::MATERIAL, broken()),
            object(
                12,
                ClassId::MATERIAL,
                material("Bare", ptr(0, 0), &[("_Detail", ptr(0, 77))]),
            ),
            object(
                20,
                ClassId::GAME_OBJECT,
                game_object("Rock", &[ptr(0, 21), ptr(0, 23), ptr(0, 22), ptr(0, 24)]),
            ),
            object(21, ClassId::TRANSFORM, transform(20, ptr(0, 0), &[])),
            object(
                22,
                ClassId::MESH_RENDERER,
                mesh_renderer_bytes(20, &distinct(), &renderer_materials),
            ),
            object(23, ClassId::MESH_FILTER, filter_bytes(20, ptr(0, 0))),
            object(
                24,
                ClassId::SKINNED_MESH_RENDERER,
                skinned_bytes(20, &distinct(), ptr(0, 0), &[ptr(0, 11)]),
            ),
        ],
        &[],
    );
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    (dir, plan)
}

/// The index of the one material named `name`.
fn material_named(plan: &ResourcesPlan, name: &str) -> usize {
    let found: Vec<_> = (0..plan.assets.len())
        .filter(|&i| matches!(&plan.assets[i], ResourceAsset::Material { material } if material.name == name))
        .collect();
    assert_eq!(found.len(), 1, "one material {name}: {:?}", plan.assets);
    found[0]
}

fn index_of(value: &Json) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}

fn as_json(plan: &ResourcesPlan, index: usize) -> Json {
    serde_json::to_value(&plan.assets[index]).unwrap()
}

fn count_of(plan: &ResourcesPlan, kind: &str) -> usize {
    (0..plan.assets.len())
        .filter(|&i| as_json(plan, i)["type"] == kind)
        .count()
}

#[test]
fn plans_a_material_once_with_every_property_its_shader_and_its_texture() {
    let (_dir, plan) = planned_resources();
    let index = material_named(&plan, "Lit");
    let lit = as_json(&plan, index);
    let shader = index_of(&lit["shader"]);
    let albedo = index_of(&lit["textures"][0]["texture"]);
    assert_eq!(as_json(&plan, shader)["name"], "Test/Lit");
    assert_eq!(as_json(&plan, albedo)["type"], "Texture2D");
    assert_eq!(
        lit,
        json!({
            "type": "Material", "name": "Lit", "shader": shader, "render_queue": 3100,
            "keywords": ["_A", "_B"], "tags": [["RenderType", "Opaque"]],
            "disabled_passes": ["ShadowCaster"],
            "floats": [["_Cutoff", 0.5]], "ints": [["_Mode", 2]],
            "colors": [["_Color", [1.0, 0.5, 0.25, 1.0]]],
            "textures": [
                {"name": "_MainTex", "texture": albedo, "scale": [2.0, 3.0], "offset": [0.5, 0.25]},
                {"name": "_Bump", "texture": null, "scale": [2.0, 3.0], "offset": [0.5, 0.25]},
            ],
        })
    );
    assert_eq!(count_of(&plan, "Shader"), 1);
    assert_eq!(count_of(&plan, "Texture2D"), 1);
}

#[test]
fn lists_a_container_material_under_its_path_and_keeps_the_class() {
    let (_dir, plan) = planned_resources();
    let index = material_named(&plan, "Lit");
    let entries: Vec<_> = plan
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.class.as_str()))
        .collect();
    assert_eq!(entries[0], ("materials/lit", "Material"));
    assert_eq!(plan.entries[0].asset, Some(index));
    assert_eq!(entries[1], ("materials/broken", "Material"));
}

#[test]
fn script_fields_bind_the_planned_material_and_null_for_one_that_did_not_plan() {
    let (dir, plan) = planned_resources();
    let index = material_named(&plan, "Lit");
    let fields = plan
        .assets
        .iter()
        .find_map(|asset| match asset {
            ResourceAsset::Script { fields, .. } => Some(fields),
            _ => None,
        })
        .expect("the script asset");
    let written: Json =
        serde_json::from_slice(&std::fs::read(dir.path().join("out").join(fields)).unwrap())
            .unwrap();
    assert_eq!(
        written,
        json!({"a": {"$asset": index}, "b": {"$asset": index}, "broken": null})
    );
}

#[test]
fn a_material_that_does_not_decode_is_an_error_entry_and_the_rest_still_plans() {
    let (_dir, plan) = planned_resources();
    let entry = plan.entries[1].asset.unwrap();
    let ResourceAsset::UnplannedMaterial { error } = &plan.assets[entry] else {
        panic!("the broken material: {:?}", plan.assets[entry])
    };
    assert!(error.contains("resources.assets"), "{error}");
    for wanted in [
        "resource main.broken: references material object 11 of file 0, which did not plan",
        "material did not plan: material object 11",
    ] {
        assert!(
            plan.differences.iter().any(|line| line.starts_with(wanted)),
            "{wanted}: {:?}",
            plan.differences
        );
    }
}

#[test]
fn a_material_without_a_shader_or_with_a_missing_texture_plans_and_says_so() {
    let (_dir, plan) = planned_resources();
    let index = material_named(&plan, "Bare");
    let bare = as_json(&plan, index);
    assert_eq!(bare["shader"], Json::Null);
    assert_eq!(bare["textures"][0]["texture"], Json::Null);
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("material Bare: references no shader")),
        "{:?}",
        plan.differences
    );
    assert!(
        plan.differences.iter().any(|line| line
            .starts_with("material Bare: texture slots _Detail bind null: references object 77")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn prefab_renderer_slots_reference_the_planned_materials() {
    let (dir, plan) = planned_resources();
    let lit = material_named(&plan, "Lit");
    let bare = material_named(&plan, "Bare");
    let prefab = plan
        .assets
        .iter()
        .find_map(|asset| match asset {
            ResourceAsset::Prefab { plan, .. } => Some(plan),
            _ => None,
        })
        .expect("the prefab");
    let scene: Json =
        serde_json::from_slice(&std::fs::read(dir.path().join("out").join(prefab)).unwrap())
            .unwrap();
    let components = &scene["objects"][0]["components"];
    assert_eq!(components[1]["materials"], json!([lit, bare, null]));
    assert_eq!(components[2]["materials"], json!([null]));
    assert!(
        plan.differences.iter().any(|line| line.starts_with(
            "prefab Rock: Rock: SkinnedMeshRenderer material 0 references material object 11"
        )),
        "{:?}",
        plan.differences
    );
}

fn write_scene(dir: &Path) {
    let root = game_object("Root", &[ptr(0, 3), ptr(0, 4)]);
    let slots = [ptr(1, 10), ptr(1, 10), ptr(1, 11), ptr(0, 0)];
    write_file(
        dir,
        "level0",
        vec![
            object(2, ClassId::GAME_OBJECT, root),
            object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[])),
            object(
                4,
                ClassId::MESH_RENDERER,
                mesh_renderer_bytes(2, &distinct(), &slots),
            ),
        ],
        &["sharedassets0.assets"],
    );
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(8, ClassId::TEXTURE_2D, texture()),
            object(9, ClassId::SHADER, lit()),
            object(
                10,
                ClassId::MATERIAL,
                material("Lit", ptr(0, 9), &[("_MainTex", ptr(0, 8))]),
            ),
            object(11, ClassId::MATERIAL, broken()),
        ],
        &[],
    );
}

#[test]
fn a_scene_plans_its_renderer_materials_with_their_shader_and_no_textures() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &layouts()).unwrap();
    let [
        PlanAsset::Shader { name, .. },
        PlanAsset::Material { material },
    ] = &plan.assets[..]
    else {
        panic!("a shader and a material: {:?}", plan.assets)
    };
    assert_eq!(name, "Test/Lit");
    assert_eq!(material.name, "Lit");
    let planned = serde_json::to_value(&plan.assets[1]).unwrap();
    assert_eq!(planned["shader"], 0);
    assert_eq!(planned["textures"][0]["texture"], Json::Null);
    assert_eq!(planned["textures"][0]["name"], "_MainTex");
    let renderer = serde_json::to_value(&plan.objects[0].components[0]).unwrap();
    assert_eq!(renderer["materials"], json!([1, 1, null, null]));
    for wanted in [
        "material Lit: texture slots _MainTex bind null: references Texture2D object 8",
        "Root: MeshRenderer material 2 references material object 11 of file 1, which did not plan",
    ] {
        assert!(
            plan.differences.iter().any(|line| line.starts_with(wanted)),
            "{wanted}: {:?}",
            plan.differences
        );
    }
}
