//! Constant buffers packed from a player variant's layout: members are named in place, so a
//! property is found by the member's name, and the member's byte offset places it.

use worldrunner_render_program::constants::{ConstantError, pack_variant_buffer};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_unity_serialized::shader_parameters::{BufferMember, ConstantBuffer, StructLayout};

const FLOAT: i32 = 0;
const INT: i32 = 1;

fn member(name: &str, kind: i32, shape: (i32, i32), array_size: i32, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind,
        rows: shape.0,
        columns: shape.1,
        is_matrix: false,
        array_size,
        offset,
    }
}

fn vector(name: &str, columns: i32, offset: i32) -> BufferMember {
    member(name, FLOAT, (1, columns), 0, offset)
}

fn matrix(name: &str, rows: i32, columns: i32, array_size: i32, offset: i32) -> BufferMember {
    BufferMember {
        is_matrix: true,
        ..member(name, FLOAT, (rows, columns), array_size, offset)
    }
}

fn buffer(members: Vec<BufferMember>, size: i32) -> ConstantBuffer {
    ConstantBuffer {
        name: "Block".to_owned(),
        size,
        members,
        structs: Vec::new(),
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn sequence<const N: usize>(first: f32) -> [f32; N] {
    std::array::from_fn(|i| first + f32::from(u8::try_from(i).unwrap()))
}

#[test]
fn members_are_found_by_name_and_land_at_their_byte_offsets() {
    let cb = buffer(
        vec![vector("_Tint", 4, 16), member("_Gain", FLOAT, (1, 1), 0, 4)],
        32,
    );
    let mut material = PropertySet::new();
    material.set("_Tint", PropertyValue::Vector([0.25, 0.5, 0.75, 1.0]));
    material.set("_Gain", PropertyValue::Float(2.0));
    let packed = pack_variant_buffer(&cb, &[&material], 0).unwrap();
    assert_eq!(
        floats(&packed.bytes),
        [0.0, 2.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0]
    );
    assert!(packed.missing.is_empty());
}

#[test]
fn a_matrix_and_an_array_share_a_buffer_with_a_vector_and_an_integer() {
    // A float4x4 at 0, a float4 array of three at 64, a float4x4 pair at 112 and an int at 240.
    let cb = buffer(
        vec![
            matrix("_World", 4, 4, 0, 0),
            member("_Weights", FLOAT, (1, 4), 3, 64),
            matrix("_Bones", 4, 4, 2, 112),
            member("_Count", INT, (1, 1), 0, 240),
        ],
        256,
    );
    let world: [f32; 16] = sequence(1.0);
    let weights = vec![[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]];
    let bones = vec![sequence(100.0), sequence(200.0)];
    let mut material = PropertySet::new();
    material.set("_World", PropertyValue::Matrix(world));
    material.set("_Weights", PropertyValue::VectorArray(weights));
    material.set("_Bones", PropertyValue::MatrixArray(bones.clone()));
    material.set("_Count", PropertyValue::Int(-7));
    let packed = pack_variant_buffer(&cb, &[&material], 0).unwrap();
    let all = floats(&packed.bytes);
    assert_eq!(all[0..16], world);
    assert_eq!(
        all[16..28],
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 0.0, 0.0, 0.0, 0.0],
        "the third array element had no value and stays zero"
    );
    assert_eq!(all[28..44], bones[0], "a matrix array element is 64 bytes");
    assert_eq!(all[44..60], bones[1]);
    assert_eq!(&packed.bytes[240..244], &(-7_i32).to_le_bytes());
}

#[test]
fn a_matrix_of_fewer_rows_writes_four_floats_per_row() {
    let cb = buffer(vec![matrix("_Rows", 3, 4, 0, 16)], 80);
    let values: [f32; 16] = sequence(1.0);
    let mut material = PropertySet::new();
    material.set("_Rows", PropertyValue::Matrix(values));
    let packed = pack_variant_buffer(&cb, &[&material], 0).unwrap();
    assert_eq!(floats(&packed.bytes)[4..16], values[..12]);
    assert_eq!(floats(&packed.bytes)[16..20], [0.0; 4]);
}

#[test]
fn a_vector_of_fewer_components_writes_only_those() {
    let cb = buffer(
        vec![
            vector("_Pair", 2, 0),
            member("_Single", FLOAT, (1, 1), 0, 8),
        ],
        16,
    );
    let mut material = PropertySet::new();
    material.set("_Pair", PropertyValue::Vector([0.5, 0.25, 9.0, 9.0]));
    material.set("_Single", PropertyValue::Float(2.0));
    let packed = pack_variant_buffer(&cb, &[&material], 0).unwrap();
    assert_eq!(floats(&packed.bytes), [0.5, 0.25, 2.0, 0.0]);
}

#[test]
fn the_first_layer_with_a_name_wins_and_missing_members_are_reported_once() {
    let cb = buffer(vec![vector("_A", 4, 0), vector("_B", 4, 16)], 32);
    let (mut material, mut globals) = (PropertySet::new(), PropertySet::new());
    material.set("_A", PropertyValue::Vector([1.0; 4]));
    globals.set("_A", PropertyValue::Vector([5.0; 4]));
    let packed = pack_variant_buffer(&cb, &[&material, &globals], 0).unwrap();
    assert_eq!(floats(&packed.bytes)[0..4], [1.0; 4]);
    assert_eq!(packed.missing, vec!["_B".to_owned()]);
    let packed = pack_variant_buffer(&cb, &[&PropertySet::new(), &globals], 0).unwrap();
    assert_eq!(floats(&packed.bytes)[0..4], [5.0; 4]);
    assert_eq!(packed.missing, vec!["_B".to_owned()]);
}

#[test]
fn the_buffer_is_at_least_the_shaders_size_rounded_to_sixteen_bytes() {
    let cb = buffer(vec![vector("_A", 4, 0)], 20);
    let len = |min| pack_variant_buffer(&cb, &[], min).unwrap().bytes.len();
    assert_eq!(len(0), 32);
    assert_eq!(len(40), 48);
}

#[test]
fn declarations_the_packer_cannot_write_are_named_errors() {
    let error =
        |member: BufferMember| pack_variant_buffer(&buffer(vec![member], 64), &[], 0).unwrap_err();
    // Only float (0) and int (1) have been measured as member kinds.
    assert_eq!(
        error(member("_Flag", 2, (1, 1), 0, 0)),
        ConstantError::UnsupportedValue {
            name: "_Flag".to_owned(),
            kind: 2
        }
    );
    assert!(matches!(
        error(member("_Ints", INT, (1, 3), 0, 0)),
        ConstantError::UnsupportedValue { kind: 1, .. }
    ));
    assert!(matches!(
        error(member("_IntArray", INT, (1, 1), 2, 0)),
        ConstantError::UnsupportedValue { kind: 1, .. }
    ));
    assert!(matches!(
        error(vector("_Wide", 5, 0)),
        ConstantError::UnsupportedValue { .. }
    ));
    assert!(matches!(
        error(member("_Rows", FLOAT, (2, 4), 0, 0)),
        ConstantError::UnsupportedValue { .. }
    ));
    assert!(
        matches!(
            error(matrix("_Narrow", 4, 3, 0, 0)),
            ConstantError::UnsupportedValue { .. }
        ),
        "a matrix with fewer than four columns is not packed until its layout is measured"
    );
    assert!(matches!(
        error(matrix("_Tall", 5, 4, 0, 0)),
        ConstantError::UnsupportedValue { .. }
    ));
}

#[test]
fn sizes_and_offsets_outside_the_buffer_are_named_errors() {
    let packed = |members, size| pack_variant_buffer(&buffer(members, size), &[], 0);
    assert_eq!(
        packed(vec![member("_A", FLOAT, (1, 4), -1, 0)], 16).unwrap_err(),
        ConstantError::NegativeSize {
            name: "_A".to_owned(),
            field: "array size",
            value: -1
        }
    );
    assert!(matches!(
        packed(vec![vector("_A", 4, 0)], -16).unwrap_err(),
        ConstantError::NegativeSize {
            field: "size",
            value: -16,
            ..
        }
    ));
    assert!(matches!(
        packed(vec![vector("_A", 4, 16)], 16).unwrap_err(),
        ConstantError::OutOfBounds { .. }
    ));
    assert!(matches!(
        packed(vec![vector("_A", 4, -4)], 16).unwrap_err(),
        ConstantError::OutOfBounds { .. }
    ));
    assert!(
        matches!(
            packed(vec![member("_A", FLOAT, (1, 4), 3, 0)], 32).unwrap_err(),
            ConstantError::OutOfBounds { .. }
        ),
        "three float4 elements need 48 bytes"
    );
}

#[test]
fn values_of_the_wrong_kind_or_length_are_named_errors() {
    let cb = buffer(
        vec![vector("_A", 4, 0), member("_W", FLOAT, (1, 4), 1, 16)],
        64,
    );
    let with = |name: &str, value| {
        let mut material = PropertySet::new();
        material.set(name, value);
        pack_variant_buffer(&cb, &[&material], 0).unwrap_err()
    };
    assert_eq!(
        with("_A", PropertyValue::Float(1.0)),
        ConstantError::WrongKind {
            property: "_A".to_owned()
        }
    );
    assert_eq!(
        with("_W", PropertyValue::VectorArray(vec![[0.0; 4]; 2])),
        ConstantError::ArrayTooLong {
            property: "_W".to_owned(),
            declared: 1,
            given: 2
        }
    );
}

#[test]
fn a_struct_is_named_not_skipped() {
    let mut cb = buffer(vec![vector("_A", 4, 0)], 64);
    cb.structs.push(StructLayout {
        name: "Item".to_owned(),
        index: 0,
        array_size: 4,
        size: 32,
        members: vec![vector("weight", 4, 16)],
    });
    assert_eq!(
        pack_variant_buffer(&cb, &[], 0).unwrap_err(),
        ConstantError::UnsupportedStruct {
            name: "Item".to_owned()
        }
    );
}
