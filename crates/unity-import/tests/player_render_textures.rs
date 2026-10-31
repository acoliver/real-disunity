//! Render textures that the resource container, materials and script fields of a player name.

use serde_json::{Value as Json, json};
use worldrunner_unity_import::player_resources::{
    ResourceAsset, ResourcesPlan, plan_player_resources,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 6] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::RENDER_TEXTURE,
    ClassId::MATERIAL,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::SHADER,
];

fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
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

fn write_file(dir: &std::path::Path, name: &str, objects: Vec<ObjectData>, externals: &[&str]) {
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

/// A 512x256 render texture with a 16-bit depth buffer, 4 samples, mips on, bilinear, repeat.
fn render_texture(name: &str) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().align();
    w.i32(0).i32(512).i32(256).i32(4).i32(-1).i32(90).i32(8);
    w.bytes(&[1, 1, 0, 0, 0, 0, 1, 0]);
    w.i32(1).i32(8).f32(0.0).i32(0).i32(0).i32(0);
    w.i32(2).i32(1).i32(2);
    w.finish()
}

/// A material of no shader with one texture slot `_MainTex` naming `texture`.
fn material(texture: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Screen").unwrap().pptr(ptr(0));
    w.i32(0).i32(0).u32(4).u8(1).u8(0).align().i32(-1);
    w.i32(0).i32(0);
    w.i32(1).string("_MainTex").unwrap().pptr(texture);
    w.floats(&[1.0, 1.0]).floats(&[0.0, 0.0]);
    w.i32(0).i32(0).i32(0).i32(0);
    w.finish()
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

fn behaviour(targets: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0)).u8(1).align().pptr(ptr(5));
    w.string("main").unwrap();
    for &target in targets {
        w.pptr(target);
    }
    w.finish()
}

fn layouts() -> ScriptLayouts {
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Config".to_owned(),
        value_type: false,
        fields: ["first", "second"]
            .iter()
            .map(|name| ScriptField {
                name: (*name).to_owned(),
                ty: FieldType::Object("UnityEngine.RenderTexture".to_owned()),
            })
            .collect(),
    }])
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(4);
    for (path, target) in [
        ("ui/target", 2),
        ("ui/broken", 3),
        ("config/main", 6),
        ("ui/screen", 7),
    ] {
        w.string(path).unwrap().pptr(PPtr {
            file_id: 1,
            path_id: target,
        });
    }
    w.i32(0);
    w.finish()
}

/// A container listing a render texture, one that does not decode and a script asset whose fields
/// name the first and a third the container does not list, and a listed material naming the first.
fn plan() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    let broken = render_texture("Broken")[..30].to_vec();
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(2, ClassId::RENDER_TEXTURE, render_texture("Target")),
            object(3, ClassId::RENDER_TEXTURE, broken),
            object(4, ClassId::RENDER_TEXTURE, render_texture("Unlisted")),
            object(5, ClassId::MONO_SCRIPT, script()),
            object(6, ClassId::MONO_BEHAVIOUR, behaviour(&[ptr(2), ptr(4)])),
            object(7, ClassId::MATERIAL, material(ptr(2))),
        ],
        &[],
    );
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    (dir, plan)
}

fn as_json(plan: &ResourcesPlan, index: usize) -> Json {
    serde_json::to_value(&plan.assets[index]).unwrap()
}

fn fields(dir: &tempfile::TempDir, plan: &ResourcesPlan) -> Json {
    let file = plan
        .assets
        .iter()
        .find_map(|asset| match asset {
            ResourceAsset::Script { fields, .. } => Some(fields),
            _ => None,
        })
        .expect("the script asset");
    serde_json::from_slice(&std::fs::read(dir.path().join("out").join(file)).unwrap()).unwrap()
}

#[test]
fn plans_a_container_render_texture_with_its_allocation_and_sampler() {
    let (_dir, plan) = plan();
    assert_eq!(plan.entries[0].path, "ui/target");
    assert_eq!(plan.entries[0].class, "RenderTexture");
    let index = plan.entries[0].asset.expect("a planned asset");
    assert_eq!(
        as_json(&plan, index),
        json!({
            "type": "RenderTexture", "name": "Target", "width": 512, "height": 256,
            "depth_format": 90, "color_format": 8, "anti_aliasing": 4, "mip_maps": true,
            "generate_mips": true, "dimension": 2, "filter_mode": 1, "aniso_level": 8,
            "wrap_u": 0, "wrap_v": 0, "wrap_w": 0,
        })
    );
}

#[test]
fn every_reference_meets_the_one_planned_render_texture() {
    let (dir, plan) = plan();
    let index = plan.entries[0].asset.unwrap();
    let written = fields(&dir, &plan);
    assert_eq!(written["first"], json!({"$asset": index}));
    let unlisted = written["second"]["$asset"].as_u64().unwrap();
    assert_ne!(unlisted, index as u64);
    assert_eq!(as_json(&plan, unlisted as usize)["name"], "Unlisted");
    let materials: Vec<_> = (0..plan.assets.len())
        .map(|i| as_json(&plan, i))
        .filter(|asset| asset["type"] == "Material")
        .collect();
    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0]["textures"][0]["texture"], index);
    let count = (0..plan.assets.len())
        .filter(|&i| as_json(&plan, i)["name"] == "Target")
        .count();
    assert_eq!(count, 1, "{:?}", plan.assets);
}

#[test]
fn a_render_texture_that_does_not_decode_is_an_error_entry_naming_the_cause() {
    let (_dir, plan) = plan();
    assert_eq!(plan.entries[1].class, "RenderTexture");
    let entry = plan.entries[1].asset.unwrap();
    let ResourceAsset::UnplannedRenderTexture { error } = &plan.assets[entry] else {
        panic!("the broken texture: {:?}", plan.assets[entry])
    };
    assert!(error.contains("render texture object 3"), "{error}");
    assert!(error.contains("resources.assets"), "{error}");
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("render texture did not plan: render texture object 3")),
        "{:?}",
        plan.differences
    );
}
