//! A player whose shaders have no Vulkan platform fails by name where its Vulkan shaders are
//! collected, rather than yielding nothing; a player with any Vulkan shader skips the others.

use std::path::{Path, PathBuf};

#[path = "support/shader_bytes.rs"]
mod shader_bytes;

use shader_bytes::shader_with_platforms;
use worldrunner_unity_import::{
    PlayerError,
    player_parameters::parameter_report,
    player_shaders::shader_report,
    player_variants::{Event, walk_variants},
};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    write::{FileContents, ObjectData},
};

/// `ShaderCompilerPlatform` of Vulkan.
const VULKAN: u32 = 18;
/// A platform other than Vulkan, with the value Metal has.
const OTHER: u32 = 14;

fn shader_object(path_id: i64, platforms: &[u32]) -> ObjectData {
    ObjectData {
        path_id,
        type_index: 0,
        data: shader_with_platforms("Test/Shader", None, &[1], platforms),
    }
}

/// A serialized file holding one shader for each platform list.
fn write_player(dir: &Path, shaders: &[&[u32]]) -> Vec<PathBuf> {
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
            .zip(1..)
            .map(|(platforms, path_id)| shader_object(path_id, platforms))
            .collect(),
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    let path = dir.join("level0");
    std::fs::write(&path, contents.write().unwrap()).unwrap();
    vec![path]
}

fn walked_shaders(paths: &[PathBuf]) -> Result<usize, PlayerError> {
    let mut shaders = 0;
    walk_variants(paths, &mut |event| {
        if matches!(event, Event::Shader(_)) {
            shaders += 1;
        }
    })?;
    Ok(shaders)
}

#[test]
fn walk_names_a_player_whose_shaders_have_no_vulkan_platform() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER]]);
    let error = walked_shaders(&paths).unwrap_err();
    assert!(matches!(error, PlayerError::NoSpirvPrograms { shaders: 1 }));
    assert_eq!(
        error.to_string(),
        "no SPIR-V programs; Metal programs unsupported: none of the player's 1 shaders has a \
         Vulkan platform"
    );
}

#[test]
fn walk_counts_every_shader_of_a_player_without_vulkan() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER], &[], &[OTHER, 4]]);
    assert!(matches!(
        walked_shaders(&paths),
        Err(PlayerError::NoSpirvPrograms { shaders: 3 })
    ));
}

#[test]
fn walk_skips_shaders_without_vulkan_beside_one_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER], &[OTHER, VULKAN]]);
    assert_eq!(walked_shaders(&paths).unwrap(), 1);
}

#[test]
fn walk_finds_no_error_in_a_player_without_shaders() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[]);
    assert_eq!(walked_shaders(&paths).unwrap(), 0);
}

#[test]
fn the_shader_report_names_a_player_without_vulkan() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER]]);
    assert!(matches!(
        shader_report(&paths),
        Err(PlayerError::NoSpirvPrograms { shaders: 1 })
    ));
}

#[test]
fn the_shader_report_counts_the_vulkan_shaders_beside_others() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER], &[VULKAN]]);
    assert_eq!(shader_report(&paths).unwrap().shaders, 1);
    let paths = write_player(dir.path(), &[]);
    assert_eq!(shader_report(&paths).unwrap().shaders, 0);
}

#[test]
fn the_parameter_report_names_a_player_without_vulkan() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER]]);
    assert!(matches!(
        parameter_report(&paths),
        Err(PlayerError::NoSpirvPrograms { shaders: 1 })
    ));
}

#[test]
fn the_parameter_report_counts_the_vulkan_shaders_beside_others() {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_player(dir.path(), &[&[OTHER], &[VULKAN]]);
    assert_eq!(parameter_report(&paths).unwrap().shaders, 1);
    let paths = write_player(dir.path(), &[]);
    assert_eq!(parameter_report(&paths).unwrap().shaders, 0);
}
