//! The render queue of a decoded shader: the first sub-shader's queue tag, `Geometry` without one.

#[path = "support/shader_bytes.rs"]
mod shader_bytes;

use shader_bytes::shader;
use worldrunner_unity_import::{player_shader_refs::shader_render_queue, player_shaders::VULKAN};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    shader::Shader,
    write::{FileContents, ObjectData},
};

fn decoded(queue: Option<(&str, &str)>, passes: &[usize]) -> Shader {
    let data = shader("Test/Queue", queue, passes);
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: vec![SerializedType {
            class_id: ClassId::SHADER,
            stripped: false,
            script_type_index: -1,
            script_id: None,
            old_type_hash: [0; 16],
            type_tree: None,
            dependencies: Vec::new(),
            script_class: None,
        }],
        objects: vec![ObjectData {
            path_id: 1,
            type_index: 0,
            data,
        }],
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("level0");
    std::fs::write(&path, contents.write().unwrap()).unwrap();
    worldrunner_unity_import::player_objects::OpenFile::open(&path)
        .unwrap()
        .decode(1)
        .unwrap()
}

#[test]
fn a_named_queue_with_an_offset_is_its_number() {
    let shader = decoded(Some(("QUEUE", "Transparent-50")), &[1]);
    assert_eq!(shader_render_queue(&shader), Ok(2950));
}

#[test]
fn a_shader_without_a_queue_tag_is_in_the_geometry_queue() {
    assert_eq!(shader_render_queue(&decoded(None, &[1])), Ok(2000));
}

#[test]
fn a_shader_without_sub_shaders_is_in_the_geometry_queue() {
    assert_eq!(shader_render_queue(&decoded(None, &[])), Ok(2000));
}

#[test]
fn a_queue_the_engine_cannot_read_is_named() {
    let shader = decoded(Some(("QUEUE", "Sometime")), &[1]);
    assert!(
        shader_render_queue(&shader)
            .unwrap_err()
            .contains("Sometime")
    );
}

#[test]
fn the_vulkan_platform_is_unitys_number_18() {
    assert_eq!(VULKAN, 18);
}
