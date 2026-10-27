//! Synthetic serialized files for tests, so no test depends on a game.

use worldrunner_unity_serialized::{
    ClassId, External, ScriptRef, SerializedType,
    write::{FileContents, ObjectData},
};

pub const SCRIPT_HASH: [u8; 16] = [7; 16];

pub fn class(class_id: ClassId) -> SerializedType {
    SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: None,
        old_type_hash: [class_id.0.to_le_bytes()[0]; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    }
}

pub fn object(path_id: i64, type_index: i32, data: Vec<u8>) -> ObjectData {
    ObjectData {
        path_id,
        type_index,
        data,
    }
}

/// A scene-like file: a game object, a transform, and a script component whose class lives
/// in another file.
pub fn scene(version: u32) -> FileContents {
    let mut behaviour = class(ClassId::MONO_BEHAVIOUR);
    behaviour.script_type_index = 0;
    behaviour.script_id = Some(SCRIPT_HASH);
    FileContents {
        version,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: vec![
            class(ClassId::GAME_OBJECT),
            class(ClassId::TRANSFORM),
            behaviour,
        ],
        objects: vec![
            object(1, 0, vec![1, 2, 3]),
            object(2, 1, vec![4; 40]),
            object(-9_000_000_000, 2, vec![5; 11]),
        ],
        scripts: vec![ScriptRef {
            file_index: 1,
            path_id: 11_500_000,
        }],
        externals: vec![External {
            guid: [3; 16],
            kind: 0,
            path: "globalgamemanagers.assets".into(),
        }],
        ref_types: Vec::new(),
        user_information: String::new(),
    }
}
