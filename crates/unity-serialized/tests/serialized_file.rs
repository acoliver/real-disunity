mod support;

use std::sync::Arc;

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

fn assert_metadata(file: &SerializedFile, contents: &FileContents) {
    assert_eq!(file.header.version, contents.version);
    assert_eq!(file.header.big_endian, contents.big_endian);
    assert_eq!(file.unity_version, contents.unity_version);
    assert_eq!(file.target_platform, contents.target_platform);
    assert_eq!(file.type_trees, contents.type_trees);
    assert_eq!(file.types(), contents.types);
    assert_eq!(file.scripts, contents.scripts);
    assert_eq!(file.externals, contents.externals);
    assert_eq!(file.ref_types, contents.ref_types);
    assert_eq!(file.user_information, contents.user_information);
}

fn assert_objects(bytes: &[u8], file: &SerializedFile, contents: &FileContents) {
    assert_eq!(file.objects().len(), contents.objects.len());
    for (read, written) in file.objects().iter().zip(&contents.objects) {
        assert_eq!(read.path_id, written.path_id);
        assert_eq!(object_bytes(bytes, file, read.path_id), written.data);
        let index = usize::try_from(written.type_index).unwrap();
        assert_eq!(read.type_index, index);
        assert_eq!(read.class_id, contents.types[index].class_id);
    }
}

/// Parses `contents` after writing it and checks that every written field reads back.
fn round_trip(contents: &FileContents) -> (Vec<u8>, SerializedFile) {
    let bytes = contents.write().unwrap();
    let file = SerializedFile::parse(&bytes).unwrap();
    assert_metadata(&file, contents);
    assert_objects(&bytes, &file, contents);
    (bytes, file)
}

#[test]
fn reads_types_objects_scripts_and_externals() {
    let (_, file) = round_trip(&scene(22));
    assert_eq!(file.header.header_len(), 48);
    let script = file.object(-9_000_000_000).unwrap();
    assert_eq!(script.class_id, ClassId::MONO_BEHAVIOUR);
    let ty = file.object_type(-9_000_000_000).unwrap();
    assert_eq!(ty.script_id, Some(SCRIPT_HASH));
    assert!(file.object(3).is_none());
    assert!(file.object_type(3).is_none());
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

fn local(text: &str) -> NodeString {
    NodeString::Local(text.into())
}

fn node(level: u8, type_name: NodeString, name: &str) -> TypeTreeNode {
    TypeTreeNode {
        version: 1,
        level,
        type_flags: 0,
        type_name,
        name: local(name),
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
        node(0, local("GameObject"), "Base"),
        node(1, NodeString::Common(222), "m_IsActive"),
    ]);
    for ty in &mut contents.types[1..] {
        ty.type_tree = Some(Vec::new());
    }
    let mut reference = class(ClassId::MONO_BEHAVIOUR);
    reference.script_type_index = 0;
    reference.script_id = Some([9; 16]);
    reference.type_tree = Some(Vec::new());
    reference.script_class = Some(ScriptClass {
        class_name: "Payload".into(),
        namespace: "Game.Data".into(),
        assembly: "Assembly-CSharp".into(),
    });
    contents.ref_types = vec![reference];
    contents
}

#[test]
fn reads_type_trees_and_reference_types() {
    let mut contents = with_type_trees(22);
    contents.types[0].dependencies = vec![0];
    let (_, file) = round_trip(&contents);
    let tree = file.types()[0].type_tree.as_deref().unwrap();
    assert!(tree[1].aligns() && !tree[0].aligns());
    // Before version 21, types carry no dependencies and reference types no class names.
    let mut contents = with_type_trees(20);
    contents.ref_types[0].script_class = None;
    round_trip(&contents);
}

/// Byte offset of the type tree string buffer in a file written from [`with_type_trees`].
fn strings_at(bytes: &[u8]) -> usize {
    let needle = b"GameObject\0Base\0";
    bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap()
}

/// Sets the name offset of node `index` of the first type's two-node tree.
fn set_name_offset(bytes: &mut [u8], index: usize, offset: u32) {
    let node = strings_at(bytes) - (2 - index) * 32;
    bytes[node + 8..node + 12].copy_from_slice(&offset.to_le_bytes());
}

#[test]
fn shares_a_name_that_many_nodes_use() {
    let mut contents = with_type_trees(22);
    let name = "x".repeat(4096);
    let shared = node(1, local("int"), &name);
    contents.types[1].type_tree = Some(vec![shared; 512]);
    let (_, file) = round_trip(&contents);
    let tree = file.types()[1].type_tree.as_deref().unwrap();
    let (NodeString::Local(first), NodeString::Local(last)) = (&tree[0].name, &tree[511].name)
    else {
        panic!("names are local");
    };
    assert!(Arc::ptr_eq(first, last));
}

#[test]
fn rejects_a_name_offset_inside_a_string_or_past_the_buffer() {
    let bytes = with_type_trees(22).write().unwrap();
    for offset in [1, 4096] {
        let mut bad = bytes.clone();
        set_name_offset(&mut bad, 0, offset);
        assert!(matches!(
            SerializedFile::parse(&bad),
            Err(SerializedError::TypeTreeString { offset: o, .. }) if o == offset
        ));
    }
}

#[test]
fn rejects_names_that_are_not_utf8() {
    let mut bytes = with_type_trees(22).write().unwrap();
    let at = strings_at(&bytes) + "GameObject\0".len();
    bytes[at] = 0xff;
    assert_eq!(
        SerializedFile::parse(&bytes),
        Err(SerializedError::InvalidString { offset: at })
    );
}

#[test]
fn needs_only_the_metadata_prefix() {
    let bytes = scene(22).write().unwrap();
    let header = Header::parse(&bytes).unwrap();
    let prefix = &bytes[..header.metadata_len().unwrap()];
    assert_eq!(SerializedFile::parse(prefix), SerializedFile::parse(&bytes));
}

#[test]
fn every_truncated_metadata_prefix_fails() {
    let bytes = with_type_trees(22).write().unwrap();
    let end = Header::parse(&bytes).unwrap().metadata_len().unwrap();
    for len in 0..end {
        assert!(
            SerializedFile::parse(&bytes[..len]).is_err(),
            "prefix of {len} bytes parsed"
        );
    }
}

#[test]
fn requires_all_of_the_declared_metadata() {
    let mut bytes = scene(22).write().unwrap();
    let end = Header::parse(&bytes).unwrap().metadata_len().unwrap();
    // Grow the declared metadata by four bytes and move the data region to make room.
    let size = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    bytes[20..24].copy_from_slice(&(size + 4).to_be_bytes());
    for field in [24..32, 32..40] {
        let value = u64::from_be_bytes(bytes[field.clone()].try_into().unwrap());
        bytes[field].copy_from_slice(&(value + 16).to_be_bytes());
    }
    assert_eq!(
        SerializedFile::parse(&bytes[..end]),
        Err(SerializedError::Truncated {
            offset: end,
            needed: 4,
            len: end
        })
    );
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
fn rejects_negative_and_inconsistent_header_sizes() {
    let mut bytes = scene(22).write().unwrap();
    bytes[24..32].copy_from_slice(&(-1_i64).to_be_bytes());
    assert_eq!(
        SerializedFile::parse(&bytes),
        Err(SerializedError::Negative {
            field: "file size",
            value: -1
        })
    );
    let mut bytes = scene(22).write().unwrap();
    bytes[32..40].copy_from_slice(&8_u64.to_be_bytes());
    assert!(matches!(
        SerializedFile::parse(&bytes),
        Err(SerializedError::InconsistentHeader { data_offset: 8, .. })
    ));
}

#[test]
fn rejects_unknown_types_and_duplicate_ids() {
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
}

#[test]
fn rejects_objects_past_the_end_of_the_file() {
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
    // Offsets near the top of the range fail the same way instead of overflowing.
    let mut bytes = scene(22).write().unwrap();
    bytes[24..32].copy_from_slice(&i64::MAX.to_be_bytes());
    bytes[32..40].copy_from_slice(&(i64::MAX - 2).to_be_bytes());
    let error = SerializedFile::parse(&bytes).unwrap_err();
    assert!(matches!(
        error,
        SerializedError::ObjectOutOfBounds { path_id: 1, .. }
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
fn rejects_an_unterminated_version_string() {
    let bytes = scene(22).write().unwrap();
    let end = 48 + "6000.3.15f1".len();
    let mut header_only = bytes[..end].to_vec();
    let metadata = u32::try_from(end - 48).unwrap();
    header_only[20..24].copy_from_slice(&metadata.to_be_bytes());
    assert_eq!(
        SerializedFile::parse(&header_only),
        Err(SerializedError::UnterminatedString { offset: 48 })
    );
}

#[test]
fn writes_script_ids_only_where_the_format_stores_them() {
    let mut contents = scene(22);
    contents.types[2].script_id = None;
    assert_eq!(
        contents.write(),
        Err(WriteError::ScriptId {
            index: 2,
            has: false
        })
    );
    let mut contents = scene(22);
    contents.types[1].script_id = Some([1; 16]);
    assert_eq!(
        contents.write(),
        Err(WriteError::ScriptId {
            index: 1,
            has: true
        })
    );
    let mut contents = with_type_trees(22);
    contents.ref_types[0].class_id = ClassId::MONO_SCRIPT;
    round_trip(&contents);
    contents.ref_types[0].script_id = None;
    assert_eq!(
        contents.write(),
        Err(WriteError::ScriptId {
            index: 0,
            has: false
        })
    );
}

#[test]
fn writing_a_reference_type_without_a_class_fails() {
    let mut contents = with_type_trees(22);
    contents.ref_types[0].script_class = None;
    assert_eq!(contents.write(), Err(WriteError::MissingScriptClass(0)));
}

#[test]
fn names_known_classes() {
    assert_eq!(ClassId::MESH_RENDERER.to_string(), "MeshRenderer");
    assert_eq!(ClassId(850_595_691).name(), Some("LightingSettings"));
    assert_eq!(ClassId(999_999).to_string(), "class 999999");
}
