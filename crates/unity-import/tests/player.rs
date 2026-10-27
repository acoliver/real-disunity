use worldrunner_unity_import::{PlayerError, is_serialized_name, read_player};
use worldrunner_unity_serialized::{
    ClassId, SerializedError, SerializedType,
    write::{FileContents, ObjectData},
};

fn file(objects: usize, padding: usize) -> Vec<u8> {
    let ty = SerializedType {
        class_id: ClassId::TEXT_ASSET,
        stripped: false,
        script_type_index: -1,
        script_id: None,
        old_type_hash: [1; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    };
    let objects = (1..=objects)
        .map(|i| ObjectData {
            path_id: i64::try_from(i).unwrap(),
            type_index: 0,
            data: vec![0; padding],
        })
        .collect();
    FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: vec![ty],
        objects,
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    }
    .write()
    .unwrap()
}

#[test]
fn recognizes_player_file_names() {
    for name in [
        "globalgamemanagers",
        "level0",
        "level12",
        "resources.assets",
        "sharedassets3.assets",
    ] {
        assert!(is_serialized_name(name), "{name}");
    }
    for name in [
        "level",
        "levelx",
        "resources.assets.resS",
        "boot.config",
        "globalgamemanagers.json",
    ] {
        assert!(!is_serialized_name(name), "{name}");
    }
}

#[test]
fn reads_every_serialized_file_of_a_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("level1"), file(2, 8)).unwrap();
    std::fs::write(dir.path().join("level0"), file(1, 8)).unwrap();
    // Object data well past the metadata is never read.
    std::fs::write(dir.path().join("resources.assets"), file(3, 1 << 20)).unwrap();
    std::fs::write(dir.path().join("resources.assets.resS"), [0xff; 64]).unwrap();
    std::fs::write(dir.path().join("boot.config"), "gfx-threading-mode=6").unwrap();
    std::fs::create_dir(dir.path().join("level9")).unwrap();
    let files = read_player(dir.path()).unwrap();
    let summary: Vec<_> = files
        .iter()
        .map(|f| (f.name.as_str(), f.file.objects().len()))
        .collect();
    assert_eq!(
        summary,
        [("level0", 1), ("level1", 2), ("resources.assets", 3)]
    );
    assert_eq!(files[2].file.objects()[2].size, 1 << 20);
}

#[test]
fn names_the_file_that_fails() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = file(1, 8);
    bytes[8..12].copy_from_slice(&17_u32.to_be_bytes());
    std::fs::write(dir.path().join("level0"), bytes).unwrap();
    let error = read_player(dir.path()).unwrap_err();
    assert!(matches!(
        &error,
        PlayerError::Serialized { path, source: SerializedError::UnsupportedVersion(17) }
            if path.ends_with("level0")
    ));
    assert!(error.to_string().contains("level0"), "{error}");
}
