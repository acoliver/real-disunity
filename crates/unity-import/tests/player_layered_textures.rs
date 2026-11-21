//! Cubemaps and 3D textures that the script fields of a player's resources name.

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

const TYPES: [ClassId; 5] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::CUBEMAP,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::TEXTURE_3D,
];

/// The streamed textures' pixels: bytes 4 to 12 of `resources.assets.resS`.
const STREAM: [u8; 16] = [0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0];

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

/// A cube of 4x4 faces in `TextureFormat` 24 (BC6H) with three mip levels, whose pixels are
/// `pixels` when inline and the first eight bytes of `STREAM[4..]` when streamed. `images` says
/// how many faces the object states.
fn cube(name: &str, images: i32, pixels: Option<&[u8]>) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().u8(1).align();
    w.i32(4).i32(4).u32(36).i32(0).i32(24).i32(3);
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
    if let Some(pixels) = pixels {
        w.byte_array(pixels).unwrap();
        w.u64(0).u32(0).string("").unwrap().align();
    } else {
        w.byte_array(&[]).unwrap();
        w.u64(4)
            .u32(8)
            .string("archive:/CAB-1/resources.assets.resS")
            .unwrap();
        w.align();
    }
    w.i32(0);
    w.finish()
}

/// A volume of 4x2x3 voxels in `GraphicsFormat` 88 with two levels, trilinear, mirrored along each
/// axis, whose pixels are `pixels` when inline and the first eight bytes of `STREAM[4..]` when
/// streamed.
fn volume(name: &str, pixels: Option<&[u8]>) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().align();
    w.i32(0).u8(1).align();
    w.i32(88).i32(4).i32(2).i32(3).i32(2).u32(116);
    w.i32(2).i32(0).f32(0.5).i32(2).i32(2).i32(2);
    w.i32(0).u8(1).align();
    if let Some(pixels) = pixels {
        w.byte_array(pixels).unwrap();
        w.u64(0).u32(0).string("").unwrap().align();
    } else {
        w.byte_array(&[]).unwrap();
        w.u64(4)
            .u32(8)
            .string("archive:/CAB-1/resources.assets.resS")
            .unwrap();
        w.align();
    }
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
    let field = |name: &str, class: &str| ScriptField {
        name: name.to_owned(),
        ty: FieldType::Object(class.to_owned()),
    };
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Config".to_owned(),
        value_type: false,
        fields: vec![
            field("sky", "UnityEngine.Cubemap"),
            field("streamed_sky", "UnityEngine.Cubemap"),
            field("sky_again", "UnityEngine.Cubemap"),
            field("broken_sky", "UnityEngine.Cubemap"),
            field("noise", "UnityEngine.Texture3D"),
            field("streamed_noise", "UnityEngine.Texture3D"),
        ],
    }])
}

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(1);
    w.string("config/main").unwrap().pptr(PPtr {
        file_id: 1,
        path_id: 6,
    });
    w.i32(0);
    w.finish()
}

/// A script asset whose fields name an inline cube, a streamed one, the inline one again, a cube
/// of five images, an inline volume and a streamed one.
fn plan() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    std::fs::write(dir.path().join("resources.assets.resS"), STREAM).unwrap();
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(2, ClassId::CUBEMAP, cube("Sky", 6, Some(&[9, 8, 7]))),
            object(3, ClassId::CUBEMAP, cube("Streamed", 6, None)),
            object(4, ClassId::CUBEMAP, cube("Broken", 5, Some(&[1]))),
            object(7, ClassId::TEXTURE_3D, volume("Noise", Some(&[6, 5]))),
            object(8, ClassId::TEXTURE_3D, volume("Fog", None)),
            object(5, ClassId::MONO_SCRIPT, script()),
            object(
                6,
                ClassId::MONO_BEHAVIOUR,
                behaviour(&[ptr(2), ptr(3), ptr(2), ptr(4), ptr(7), ptr(8)]),
            ),
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

fn index_of(written: &Json, field: &str) -> usize {
    usize::try_from(written[field]["$asset"].as_u64().unwrap()).unwrap()
}

#[test]
fn an_inline_cube_keeps_the_description_of_a_2d_texture_and_writes_its_bytes() {
    let (dir, plan) = plan();
    let index = index_of(&fields(&dir, &plan), "sky");
    let asset = as_json(&plan, index);
    let mut expected = json!({
        "type": "Cubemap", "name": "Sky", "width": 4, "height": 4, "format": 24,
        "mip_count": 3, "readable": true, "color_space": 1,
        "filter_mode": 2, "aniso_level": 8, "mip_bias": 0.5,
        "wrap_u": 1, "wrap_v": 1, "wrap_w": 1,
    });
    expected["data"] = json!({"file": format!("resources/{index}.bin"), "offset": 0, "size": 3});
    assert_eq!(asset, expected);
    let written = std::fs::read(
        dir.path()
            .join("out")
            .join(format!("resources/{index}.bin")),
    );
    assert_eq!(written.unwrap(), [9, 8, 7]);
}

#[test]
fn a_streamed_cube_names_its_range_of_the_resource_file() {
    let (dir, plan) = plan();
    let asset = as_json(&plan, index_of(&fields(&dir, &plan), "streamed_sky"));
    assert_eq!(asset["type"], "Cubemap");
    assert_eq!(asset["name"], "Streamed");
    assert_eq!(asset["data"]["offset"], 4);
    assert_eq!(asset["data"]["size"], 8);
    let file = asset["data"]["file"].as_str().unwrap();
    assert!(file.ends_with("resources.assets.resS"), "{file}");
}

#[test]
fn two_references_to_one_cube_share_one_asset() {
    let (dir, plan) = plan();
    let written = fields(&dir, &plan);
    assert_eq!(index_of(&written, "sky"), index_of(&written, "sky_again"));
    assert_ne!(
        index_of(&written, "sky"),
        index_of(&written, "streamed_sky")
    );
    let count = (0..plan.assets.len())
        .filter(|&i| as_json(&plan, i)["type"] == "Cubemap")
        .count();
    assert_eq!(count, 2);
}

#[test]
fn a_cube_that_is_not_six_faces_binds_null_and_is_noted() {
    let (dir, plan) = plan();
    assert_eq!(fields(&dir, &plan)["broken_sky"], Json::Null);
    assert!(
        plan.differences
            .iter()
            .any(|line| line.contains("cubemap Broken: 5 images of 4x4 are not six square faces")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn a_volume_keeps_its_size_depth_and_graphics_format_and_names_its_bytes() {
    let (dir, plan) = plan();
    let written = fields(&dir, &plan);
    let index = index_of(&written, "noise");
    let asset = as_json(&plan, index);
    let mut expected = json!({
        "type": "Texture3D", "name": "Noise", "width": 4, "height": 2, "depth": 3,
        "graphics_format": 88, "mip_count": 2, "readable": true, "color_space": 0,
        "filter_mode": 2, "aniso_level": 0, "mip_bias": 0.5,
        "wrap_u": 2, "wrap_v": 2, "wrap_w": 2,
    });
    expected["data"] = json!({"file": format!("resources/{index}.bin"), "offset": 0, "size": 2});
    assert_eq!(asset, expected);
    let streamed = as_json(&plan, index_of(&written, "streamed_noise"));
    assert_eq!(streamed["name"], "Fog");
    assert_eq!(
        (&streamed["data"]["offset"], &streamed["data"]["size"]),
        (&json!(4), &json!(8))
    );
}
