use worldrunner_unity_serialized::{
    SerializedError,
    shader_parameters::{
        BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, StructLayout,
        VariantParameters, platform_parameters, read_bind_channels, read_parameters,
    },
    shader_program::{PlayerSubProgram, Program},
    shader_records::{RECORD_VERSION, SPIRV_PROGRAM},
};

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A length-prefixed string padded to four bytes.
fn string(text: &str) -> Vec<u8> {
    let mut bytes = words(&[u32::try_from(text.len()).unwrap()]);
    bytes.extend(text.as_bytes());
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes
}

fn member(name: &str, fields: [u32; 6]) -> Vec<u8> {
    let mut bytes = string(name);
    bytes.extend(words(&fields));
    bytes
}

/// A packed slot word.
fn slot(binding: u32, set: u32, stages: u32) -> u32 {
    binding | set << 16 | stages << 26
}

fn reason(error: SerializedError) -> &'static str {
    match error {
        SerializedError::InvalidShaderProgram { reason, .. } => reason,
        other => panic!("unexpected error {other}"),
    }
}

/// An unnamed empty buffer, a buffer with a matrix, a vector and a struct, and one binding of
/// each kind.
fn blob() -> Vec<u8> {
    let mut bytes = words(&[RECORD_VERSION, 2]);
    bytes.extend(string(""));
    bytes.extend(words(&[0, 0, 0]));
    bytes.extend(string("Block"));
    bytes.extend(words(&[96, 2]));
    bytes.extend(member("mat", [0, 4, 4, 1, 0, 0]));
    bytes.extend(member("vec", [1, 1, 3, 0, 5, 64]));
    bytes.extend(words(&[1]));
    bytes.extend(string("Item"));
    bytes.extend(words(&[0, 4, 32, 1]));
    bytes.extend(member("weight", [0, 1, 1, 0, 0, 16]));
    bytes.extend(words(&[5]));
    bytes.extend(string("Tex"));
    bytes.extend(words(&[0, slot(2, 0, 2), slot(3, 0, 2), 8]));
    bytes.extend(string("Block"));
    bytes.extend(words(&[1, slot(1, 1, 3), 0]));
    bytes.extend(string("Buf"));
    bytes.extend(words(&[2, slot(4, 0, 1), 1]));
    bytes.extend(string("Img"));
    bytes.extend(words(&[3, slot(5, 2, 2), 1]));
    bytes.extend(string(""));
    bytes.extend(words(&[4, slot(3, 0, 2), 85]));
    bytes
}

fn scalar(name: &str, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind: 0,
        rows: 1,
        columns: 1,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

fn constant_buffers() -> Vec<ConstantBuffer> {
    let matrix = BufferMember {
        rows: 4,
        columns: 4,
        is_matrix: true,
        ..scalar("mat", 0)
    };
    let array = BufferMember {
        kind: 1,
        columns: 3,
        array_size: 5,
        ..scalar("vec", 64)
    };
    vec![
        ConstantBuffer {
            name: String::new(),
            size: 0,
            members: Vec::new(),
            structs: Vec::new(),
        },
        ConstantBuffer {
            name: "Block".to_owned(),
            size: 96,
            members: vec![matrix, array],
            structs: vec![StructLayout {
                name: "Item".to_owned(),
                index: 0,
                array_size: 4,
                size: 32,
                members: vec![scalar("weight", 16)],
            }],
        },
    ]
}

fn parameters() -> VariantParameters {
    let slot = |binding, set, stages| DescriptorSlot {
        binding,
        set,
        stages,
    };
    let bind = |name: &str, slot, kind| ResourceBinding {
        name: name.to_owned(),
        slot,
        kind,
    };
    let texture = ResourceKind::Texture {
        sampler: Some(slot(3, 0, 2)),
        dimension: 8,
    };
    VariantParameters {
        constant_buffers: constant_buffers(),
        bindings: vec![
            bind("Tex", slot(2, 0, 2), texture),
            bind("Block", slot(1, 1, 3), ResourceKind::ConstantBuffer),
            bind("Buf", slot(4, 0, 1), ResourceKind::Buffer),
            bind("Img", slot(5, 2, 2), ResourceKind::StorageImage),
            bind("", slot(3, 0, 2), ResourceKind::Sampler { state: 85 }),
        ],
    }
}

#[test]
fn reads_buffers_and_bindings_and_consumes_the_entry_exactly() {
    assert_eq!(read_parameters(&blob()).unwrap(), parameters());
}

#[test]
fn a_texture_without_a_sampler_has_none() {
    let mut bytes = words(&[RECORD_VERSION, 0, 1]);
    bytes.extend(string("Tex"));
    bytes.extend(words(&[0, slot(0, 0, 1), u32::MAX, 4]));
    let read = read_parameters(&bytes).unwrap();
    assert_eq!(
        read.bindings[0].kind,
        ResourceKind::Texture {
            sampler: None,
            dimension: 4,
        }
    );
}

#[test]
fn every_truncation_of_the_entry_fails() {
    let bytes = blob();
    for len in 0..bytes.len() {
        assert!(
            read_parameters(&bytes[..len]).is_err(),
            "{len} of {} bytes read",
            bytes.len()
        );
    }
}

#[test]
fn bytes_after_the_last_binding_fail() {
    let mut bytes = blob();
    bytes.extend([0; 4]);
    assert_eq!(
        reason(read_parameters(&bytes).unwrap_err()),
        "the parameters have bytes after the last binding"
    );
}

#[test]
fn an_unknown_version_fails() {
    let mut bytes = blob();
    bytes[..4].copy_from_slice(&(RECORD_VERSION - 1).to_le_bytes());
    assert_eq!(
        reason(read_parameters(&bytes).unwrap_err()),
        "the parameters version is not 202012090"
    );
}

#[test]
fn an_unknown_binding_kind_fails() {
    let mut bytes = words(&[RECORD_VERSION, 0, 1]);
    bytes.extend(string("X"));
    bytes.extend(words(&[9, slot(0, 0, 1), 0]));
    assert_eq!(
        reason(read_parameters(&bytes).unwrap_err()),
        "a resource binding has an unknown kind"
    );
}

#[test]
fn a_binding_whose_last_word_is_not_the_one_its_kind_has_fails() {
    for (tag, last) in [(1, 1), (2, 0), (3, 0)] {
        let mut bytes = words(&[RECORD_VERSION, 0, 1]);
        bytes.extend(string("X"));
        bytes.extend(words(&[tag, slot(0, 0, 1), last]));
        assert_eq!(
            reason(read_parameters(&bytes).unwrap_err()),
            "a resource binding's last word is not the one its kind has",
            "kind {tag}"
        );
    }
}

#[test]
fn a_matrix_flag_other_than_zero_or_one_fails() {
    let mut bytes = words(&[RECORD_VERSION, 1]);
    bytes.extend(string("B"));
    bytes.extend(words(&[16, 1]));
    bytes.extend(member("m", [0, 4, 4, 2, 0, 0]));
    bytes.extend(words(&[0, 0]));
    assert_eq!(
        reason(read_parameters(&bytes).unwrap_err()),
        "a buffer member's matrix flag is not 0 or 1"
    );
}

#[test]
fn a_count_larger_than_the_entry_fails() {
    let bytes = words(&[RECORD_VERSION, 1_000_000]);
    assert!(matches!(
        read_parameters(&bytes).unwrap_err(),
        SerializedError::InvalidCount { .. }
    ));
}

#[test]
fn a_name_that_is_not_utf8_fails() {
    let mut bytes = words(&[RECORD_VERSION, 1, 2]);
    bytes.extend([0xff, 0xfe, 0, 0]);
    bytes.extend(words(&[0, 0, 0, 0, 0]));
    assert!(matches!(
        read_parameters(&bytes).unwrap_err(),
        SerializedError::InvalidString { .. }
    ));
}

#[test]
fn unpacks_a_slot_word() {
    assert_eq!(
        DescriptorSlot::unpack(slot(0xabcd, 0x2a5, 3)),
        DescriptorSlot {
            binding: 0xabcd,
            set: 0x2a5,
            stages: DescriptorSlot::VERTEX | DescriptorSlot::FRAGMENT,
        }
    );
}

#[test]
fn reads_a_vertex_input_table() {
    let tail = words(&[0b1_0001, 2, 0, 13, 4, 14]);
    let channels = read_bind_channels(&tail).unwrap();
    assert_eq!(channels.source_map, 0b1_0001);
    assert_eq!(channels.channels, [[0, 13], [4, 14]]);
    let empty = read_bind_channels(&words(&[0, 0])).unwrap();
    assert!(empty.channels.is_empty());
}

#[test]
fn every_truncation_of_a_vertex_input_table_fails() {
    let tail = words(&[0b11, 2, 0, 13, 1, 14]);
    for len in 0..tail.len() {
        assert!(
            read_bind_channels(&tail[..len]).is_err(),
            "{len} bytes read"
        );
    }
}

#[test]
fn a_vertex_input_table_with_extra_bytes_or_a_wide_channel_fails() {
    let mut tail = words(&[1, 1, 0, 13]);
    tail.extend([0; 4]);
    assert_eq!(
        reason(read_bind_channels(&tail).unwrap_err()),
        "the vertex input table has extra bytes"
    );
    assert_eq!(
        reason(read_bind_channels(&words(&[1, 1, 0, 300])).unwrap_err()),
        "a vertex input channel does not fit in a byte"
    );
}

/// A chunk holding a table of `entries` followed by their bytes.
fn chunk(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut at = 4 + 12 * entries.len();
    let mut table = words(&[u32::try_from(entries.len()).unwrap()]);
    for entry in entries {
        table.extend(words(&[
            u32::try_from(at).unwrap(),
            u32::try_from(entry.len()).unwrap(),
            0,
        ]));
        at += entry.len();
    }
    table.extend(entries.concat());
    table
}

fn variant(gpu_program_type: i8) -> PlayerSubProgram {
    PlayerSubProgram {
        blob_index: 0,
        keyword_indices: Vec::new(),
        requirements: 0,
        gpu_program_type,
    }
}

fn program(variants: Vec<PlayerSubProgram>, blobs: Vec<u32>) -> Program {
    Program {
        player_sub_programs: vec![Vec::new(), variants],
        parameter_blob_indices: vec![Vec::new(), blobs],
        ..Program::default()
    }
}

#[test]
fn each_variant_of_a_platform_gets_the_parameters_at_its_index() {
    let mut other = words(&[RECORD_VERSION, 0]);
    other.extend(words(&[0]));
    let chunks = vec![chunk(&[vec![0; 8], blob(), other])];
    let (mut first, mut last) = (variant(SPIRV_PROGRAM), variant(SPIRV_PROGRAM));
    (first.blob_index, last.blob_index) = (10, 11);
    let program = program(vec![first, variant(18), last], vec![1, 99, 2]);
    let found = platform_parameters(&program, &(SPIRV_PROGRAM..=SPIRV_PROGRAM), &chunks).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].0.blob_index, 10);
    assert_eq!(found[0].1, parameters());
    assert_eq!(found[1].0.blob_index, 11);
    assert!(found[1].1.bindings.is_empty());
}

#[test]
fn parameter_lists_that_do_not_pair_with_the_variants_fail() {
    let chunks = vec![chunk(&[blob()])];
    let types = SPIRV_PROGRAM..=SPIRV_PROGRAM;
    let short = program(vec![variant(SPIRV_PROGRAM)], Vec::new());
    assert_eq!(
        reason(platform_parameters(&short, &types, &chunks).unwrap_err()),
        "a parameter list is not as long as its variant list"
    );
    let mut fewer_lists = program(vec![variant(SPIRV_PROGRAM)], vec![0]);
    fewer_lists.parameter_blob_indices.pop();
    assert_eq!(
        reason(platform_parameters(&fewer_lists, &types, &chunks).unwrap_err()),
        "the parameter lists do not match the variant lists"
    );
}

#[test]
fn a_variant_naming_a_missing_entry_fails() {
    let chunks = vec![chunk(&[blob()])];
    let program = program(vec![variant(SPIRV_PROGRAM)], vec![7]);
    assert_eq!(
        reason(
            platform_parameters(&program, &(SPIRV_PROGRAM..=SPIRV_PROGRAM), &chunks).unwrap_err()
        ),
        "a variant names a parameter entry that does not exist"
    );
}
