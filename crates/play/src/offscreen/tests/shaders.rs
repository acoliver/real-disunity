//! Loading a shader from a synthetic player data directory.

use std::path::Path;

use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    write::{FileContents, ObjectData},
};
use worldrunner_world_simulation::graphics::ShaderSource;

use super::super::shaders::{ShaderLoadError, ShaderLoader};

#[path = "../../../../unity-import/tests/support/shader_bytes.rs"]
mod shader_bytes;

/// `ShaderCompilerPlatform` of Vulkan, and of another platform.
const VULKAN: u32 = 18;
const OTHER: u32 = 14;

fn write(directory: &Path, file: &str, shaders: &[(i64, &[u32], usize, Option<(&str, &str)>)]) {
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
        objects: shaders
            .iter()
            .map(|&(path_id, platforms, passes, queue)| ObjectData {
                path_id,
                type_index: 0,
                data: shader_bytes::shader_with_platforms(
                    "Test/Shader",
                    queue,
                    &[passes],
                    platforms,
                ),
            })
            .collect(),
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    let path = directory.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents.write().unwrap()).unwrap();
}

fn source(file: &str, path_id: i64) -> ShaderSource {
    ShaderSource {
        file: file.to_owned(),
        path_id,
    }
}

#[test]
fn a_shader_loads_from_the_file_and_object_its_source_names() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "level0",
        &[
            (3, &[OTHER, VULKAN], 2, None),
            (4, &[VULKAN], 5, Some(("QUEUE", "Transparent"))),
        ],
    );
    let mut loader = ShaderLoader::new(directory.path());
    let first = loader.load(&source("level0", 3)).unwrap();
    assert_eq!(first.passes, 2);
    assert_eq!(first.queue, 2000, "no queue tag is the geometry queue");
    let second = loader.load(&source("level0", 4)).unwrap();
    assert_eq!((second.passes, second.queue), (5, 3000));
}

#[test]
fn a_file_in_a_subdirectory_is_named_with_forward_slashes() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "Resources/shaders.assets",
        &[(1, &[VULKAN], 1, None)],
    );
    let mut loader = ShaderLoader::new(directory.path());
    assert_eq!(
        loader
            .load(&source("Resources/shaders.assets", 1))
            .unwrap()
            .passes,
        1
    );
}

#[test]
fn a_source_is_decoded_once_however_many_shaders_name_it() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "level0", &[(3, &[VULKAN], 2, None)]);
    let mut loader = ShaderLoader::new(directory.path());
    let wanted = source("level0", 3);
    loader.load(&wanted).unwrap();
    // The file is gone, so a second decode would fail; the cached one serves.
    std::fs::remove_file(directory.path().join("level0")).unwrap();
    assert_eq!(loader.load(&wanted).unwrap().passes, 2);
    assert_eq!(loader.sources(), 1);
}

#[test]
fn a_source_that_failed_is_not_read_again() {
    let directory = tempfile::tempdir().unwrap();
    let mut loader = ShaderLoader::new(directory.path());
    let wanted = source("missing", 1);
    assert!(matches!(
        loader.load(&wanted),
        Err(ShaderLoadError::Read(_))
    ));
    write(directory.path(), "missing", &[(1, &[VULKAN], 1, None)]);
    assert!(
        matches!(loader.load(&wanted), Err(ShaderLoadError::Read(_))),
        "the failure is kept"
    );
    assert_eq!(loader.sources(), 1);
}

#[test]
fn a_shader_with_no_vulkan_platform_is_named() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "level0", &[(1, &[OTHER], 1, None)]);
    let mut loader = ShaderLoader::new(directory.path());
    let error = loader.load(&source("level0", 1)).err().unwrap();
    assert!(matches!(error, ShaderLoadError::NoVulkanPlatform));
    assert_eq!(error.name(), "NoVulkanPlatform");
}

#[test]
fn an_object_that_is_not_in_the_file_is_a_read_error() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "level0", &[(1, &[VULKAN], 1, None)]);
    let mut loader = ShaderLoader::new(directory.path());
    assert!(matches!(
        loader.load(&source("level0", 99)),
        Err(ShaderLoadError::Read(_))
    ));
}

#[test]
fn a_path_that_leaves_the_data_directory_is_refused_before_it_is_opened() {
    let directory = tempfile::tempdir().unwrap();
    let mut loader = ShaderLoader::new(directory.path().join("data"));
    for file in ["../level0", "/etc/passwd", "a/../../b", "..", "a\\b", ""] {
        assert!(
            matches!(
                loader.load(&source(file, 1)),
                Err(ShaderLoadError::OutsideDataDirectory { .. })
            ),
            "{file:?}"
        );
    }
}
