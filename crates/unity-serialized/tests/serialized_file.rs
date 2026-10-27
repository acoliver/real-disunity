mod support;

use support::{SCRIPT_HASH, class, scene};
use worldrunner_unity_serialized::{
    ClassId, Header, NodeString, ScriptClass, SerializedError, SerializedFile, TypeTreeNode,
    write::{FileContents, WriteError},
};

fn object_bytes<'a>(bytes: &'a [u8], file: &SerializedFile, path_id: i64) -> &'a [u8] {
    let object = file.object(path_id).expect("object is listed");
    let start = usize::try_from(object.offset).unwrap();
    &bytes[start..start + object.size as usize]
}

/// Parses `contents` after writing it and checks that every written field reads back.
fn round_trip(contents: &FileContents) -> (Vec<u8>, SerializedFile) {
    let bytes = contents.write().unwrap();
    let file = SerializedFile::parse(&bytes).unwrap();
    assert_eq!(file.header.version, contents.version);
    assert_eq!(file.header.big_endian, contents.big_endian);
    assert_eq!(file.unity_version, contents.unity_version);
    assert_eq!(file.target_platform, contents.target_platform);
    assert_eq!(file.type_trees, contents.type_trees);
    assert_eq!(file.types, contents.types);
    assert_eq!(file.scripts, contents.scripts);
    assert_eq!(file.externals, contents.externals);
    assert_eq!(file.ref_types, contents.ref_types);
    assert_eq!(file.user_information, contents.user_information);
    assert_eq!(file.objects().len(), contents.objects.len());
    for (read, written) in file.objects().iter().zip(&contents.objects) {
        assert_eq!(read.path_id, written.path_id);
        assert_eq!(object_bytes(&bytes, &file, read.path_id), written.data);
        let index = usize::try_from(written.type_index).unwrap();
        assert_eq!(read.type_index, index);
        assert_eq!(read.class_id, contents.types[index].class_id);
    }
    (bytes, file)
}

#[test]
fn reads_types_objects_scripts_and_externals() {
    let (_, file) = round_trip(&scene(22));
    assert_eq!(file.header.header_len(), 48);
    let script = file.object(-9_000_000_000).unwrap();
    assert_eq!(script.class_id, ClassId::MONO_BEHAVIOUR);
    assert_eq!(file.object_type(script).script_id, Some(SCRIPT_HASH));
    assert!(file.object(3).is_none());
}

#[test]
fn reads_legacy_headers_with_32_bit_offsets() {
    for version in 19..=21 {
        let (_, file) = round_trip(&scene(version));
        assert_eq!(file.header.header_len(), 20);
    }
}

#[test]
fn reads_big_endian_metadata() {
    let mut contents = scene(22);
    contents.big_endian = true;
    contents.user_information = "built by a test".into();
    round_trip(&contents);
}

fn node(level: u8, type_name: NodeString, name: &str) -> TypeTreeNode {
    TypeTreeNode {
        version: 1,
        level,
        type_flags: 0,
        type_name,
        name: NodeString::Local(name.into()),
        byte_size: if level == 0 { -1 } else { 1 },
        index: i32::from(level),
        meta_flags: if level == 1 { 0x4000 } else { 0 },
        ref_type_hash: 0,
    }
}

fn with_type_trees(version: u32) -> FileContents {
    let mut contents = scene(version);
    contents.type_trees = true;
    contents.types[0].type_tree = Some(vec![
        node(0, NodeString::Local("GameObject".into()), "Base"),
        node(1, NodeString::Common(222), "m_IsActive"),
    ]);
    for ty in &mut contents.types[1..] {
        ty.type_tree = Some(Vec::new());
    }
    let mut reference = class(ClassId::MONO_BEHAVIOUR);
    reference.script_type_index = 0;
    reference.script_id = Some([9; 16]);
    reference.type_tree = Some(Vec::new());
    contents.ref_types = vec![reference];
    contents
}

#[test]
fn reads_type_trees_and_reference_types() {
    let mut contents = with_type_trees(22);
    contents.types[0].dependencies = vec![0];
    contents.ref_types[0].script_class = Some(ScriptClass {
        class_name: "Payload".into(),
        namespace: "Game.Data".into(),
        assembly: "Assembly-CSharp".into(),
    });
    let (_, file) = round_trip(&contents);
    let tree = file.types[0].type_tree.as_deref().unwrap();
    assert!(tree[1].aligns() && !tree[0].aligns());
    // Before version 21, types carry no dependencies and reference types no class names.
    round_trip(&with_type_trees(20));
}

#[test]
fn needs_only_the_metadata_prefix() {
    let bytes = scene(22).write().unwrap();
    let header = Header::parse(&bytes).unwrap();
    let prefix = &bytes[..usize::try_from(header.metadata_end()).unwrap()];
    assert_eq!(SerializedFile::parse(prefix), SerializedFile::parse(&bytes));
}

#[test]
fn every_truncated_metadata_prefix_fails() {
    let mut contents = with_type_trees(22);
    contents.ref_types[0].script_class = Some(ScriptClass {
        class_name: "A".into(),
        namespace: String::new(),
        assembly: "B".into(),
    });
    let bytes = contents.write().unwrap();
    let end = usize::try_from(Header::parse(&bytes).unwrap().metadata_end()).unwrap();
    for len in 0..end {
        assert!(
            SerializedFile::parse(&bytes[..len]).is_err(),
            "prefix of {len} bytes parsed"
        );
    }
}

#[test]
fn rejects_unsupported_versions() {
    for version in [18_u32, 23] {
        let mut bytes = scene(22).write().unwrap();
        bytes[8..12].copy_from_slice(&version.to_be_bytes());
        assert_eq!(
            SerializedFile::parse(&bytes),
            Err(SerializedError::UnsupportedVersion(version))
        );
        assert_eq!(
            scene(version).write(),
            Err(WriteError::UnsupportedVersion(version))
        );
    }
}

#[test]
fn rejects_a_data_offset_inside_the_metadata() {
    let mut bytes = scene(22).write().unwrap();
    bytes[32..40].copy_from_slice(&8_u64.to_be_bytes());
    assert!(matches!(
        SerializedFile::parse(&bytes),
        Err(SerializedError::InconsistentHeader { data_offset: 8, .. })
    ));
}

#[test]
fn rejects_unknown_types_duplicate_ids_and_objects_past_the_end() {
    let mut contents = scene(22);
    contents.objects[1].type_index = 3;
    let error = SerializedFile::parse(&contents.write().unwrap()).unwrap_err();
    assert_eq!(
        error,
        SerializedError::UnknownType {
            path_id: 2,
            index: 3,
            types: 3
        }
    );

    let mut contents = scene(22);
    contents.objects[2].path_id = 1;
    let error = SerializedFile::parse(&contents.write().unwrap()).unwrap_err();
    assert_eq!(error, SerializedError::DuplicatePathId(1));

    let mut bytes = scene(22).write().unwrap();
    let file_size = u64::try_from(bytes.len() - 8).unwrap();
    bytes[24..32].copy_from_slice(&file_size.to_be_bytes());
    let error = SerializedFile::parse(&bytes).unwrap_err();
    assert!(matches!(
        error,
        SerializedError::ObjectOutOfBounds {
            path_id: -9_000_000_000,
            ..
        }
    ));
}

/// Offset of the type count in [`scene`] files: after the version string, platform and flag.
const TYPE_COUNT_AT: usize = 48 + "6000.3.15f1".len() + 1 + 4 + 1;

#[test]
fn rejects_counts_the_metadata_cannot_hold() {
    let mut bytes = scene(22).write().unwrap();
    for count in [1_000_000_i32, -1] {
        bytes[TYPE_COUNT_AT..TYPE_COUNT_AT + 4].copy_from_slice(&count.to_le_bytes());
        let error = SerializedFile::parse(&bytes).unwrap_err();
        assert!(
            matches!(error, SerializedError::InvalidCount { what: "type", count: c, .. } if c == count),
            "{error}"
        );
    }
}

#[test]
fn rejects_a_boolean_that_is_neither_zero_nor_one() {
    let mut bytes = scene(22).write().unwrap();
    let flag_at = TYPE_COUNT_AT - 1;
    bytes[flag_at] = 2;
    assert_eq!(
        SerializedFile::parse(&bytes),
        Err(SerializedError::InvalidBool {
            offset: flag_at,
            value: 2
        })
    );
}

#[test]
fn writing_a_reference_type_without_a_class_fails() {
    let contents = with_type_trees(22);
    assert_eq!(contents.write(), Err(WriteError::MissingScriptClass(0)));
}

#[test]
fn names_known_classes() {
    assert_eq!(ClassId::MESH_RENDERER.to_string(), "MeshRenderer");
    assert_eq!(ClassId(850_595_691).name(), Some("LightingSettings"));
    assert_eq!(ClassId(999_999).to_string(), "class 999999");
}
