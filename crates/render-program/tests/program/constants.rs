//! Constant buffer contents packed from named property values at Unity's byte offsets.

use worldrunner_render_program::constants::{ConstantError, pack_constant_buffer};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_unity_serialized::shader_program::{ConstantBuffer, StructParam, ValueParam};

const FLOAT: i8 = 0;
const INT: i8 = 1;
const BOOL: i8 = 2;
const HALF: i8 = 3;
const UINT: i8 = 5;

fn names() -> Vec<(String, i32)> {
    [
        "_Tint", "_Offset", "_Matrix", "_Weights", "_Count", "_Flag", "_Half",
    ]
    .iter()
    .zip(0..)
    .map(|(name, index)| ((*name).to_owned(), index))
    .collect()
}

fn param(name_index: i32, offset: i32, array_size: i32, kind: i8, dimension: i8) -> ValueParam {
    ValueParam {
        name_index,
        index: offset,
        array_size,
        kind,
        dimension,
    }
}

fn buffer(vectors: Vec<ValueParam>, matrices: Vec<ValueParam>, size: i32) -> ConstantBuffer {
    ConstantBuffer {
        name_index: 100,
        matrices,
        vectors,
        structs: Vec::new(),
        size,
        partial: false,
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn a_vector_lands_at_its_byte_offset_and_the_rest_stays_zero() {
    let cb = buffer(vec![param(0, 16, 1, FLOAT, 4)], vec![], 32);
    let mut material = PropertySet::new();
    material.set("_Tint", PropertyValue::Vector([0.25, 0.5, 0.75, 1.0]));
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap();
    assert_eq!(
        floats(&packed.bytes),
        [0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0]
    );
    assert!(packed.missing.is_empty());
}

#[test]
fn a_vector_of_fewer_components_writes_only_those() {
    let cb = buffer(
        vec![param(0, 0, 1, FLOAT, 2), param(1, 8, 1, FLOAT, 1)],
        vec![],
        16,
    );
    let mut material = PropertySet::new();
    material.set("_Tint", PropertyValue::Vector([0.5, 0.25, 9.0, 9.0]));
    material.set("_Offset", PropertyValue::Float(2.0));
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap();
    assert_eq!(floats(&packed.bytes), [0.5, 0.25, 2.0, 0.0]);
}

#[test]
fn a_matrix_writes_four_floats_per_row() {
    let cb = buffer(vec![], vec![param(2, 0, 1, FLOAT, 3)], 48);
    let values: [f32; 16] = std::array::from_fn(|i| f32::from(u8::try_from(i).unwrap()));
    let mut material = PropertySet::new();
    material.set("_Matrix", PropertyValue::Matrix(values));
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap();
    assert_eq!(floats(&packed.bytes), values[..12]);
}

#[test]
fn array_elements_sit_sixteen_bytes_apart() {
    let cb = buffer(vec![param(3, 0, 3, FLOAT, 2)], vec![], 48);
    let mut material = PropertySet::new();
    material.set(
        "_Weights",
        PropertyValue::VectorArray(vec![[1.0, 2.0, 0.0, 0.0], [3.0, 4.0, 0.0, 0.0]]),
    );
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap();
    assert_eq!(
        floats(&packed.bytes),
        [1.0, 2.0, 0.0, 0.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        "the third element had no value and stays zero"
    );
}

#[test]
fn integers_and_booleans_are_written_as_integers() {
    let cb = buffer(
        vec![param(4, 0, 1, INT, 1), param(5, 4, 1, BOOL, 1)],
        vec![],
        16,
    );
    let mut material = PropertySet::new();
    material.set("_Count", PropertyValue::Int(-3));
    material.set("_Flag", PropertyValue::Int(1));
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap();
    assert_eq!(&packed.bytes[0..4], &(-3_i32).to_le_bytes());
    assert_eq!(&packed.bytes[4..8], &1_u32.to_le_bytes());
}

/// The first four bytes of the buffer a single scalar member packs to, for `value`.
fn scalar_bytes(kind: i8, value: PropertyValue) -> Result<[u8; 4], ConstantError> {
    let cb = buffer(vec![param(4, 0, 1, kind, 1)], vec![], 16);
    let mut material = PropertySet::new();
    material.set("_Count", value);
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0)?;
    Ok(packed.bytes[..4].try_into().unwrap())
}

fn wrong_kind() -> ConstantError {
    ConstantError::WrongKind {
        property: "_Count".to_owned(),
    }
}

#[test]
fn an_integer_set_on_a_float_member_is_written_as_that_number_while_a_float_can_hold_it_exactly() {
    let float = |value| scalar_bytes(FLOAT, PropertyValue::Int(value)).map(f32::from_le_bytes);
    assert_eq!(float(3), Ok(3.0));
    assert_eq!(float(-7), Ok(-7.0));
    assert_eq!(float(0), Ok(0.0));
    assert_eq!(float(1 << 24), Ok(16_777_216.0));
    assert_eq!(float((1 << 24) + 1), Err(wrong_kind()));
    assert_eq!(float(-(1 << 24) - 1), Err(wrong_kind()));
}

#[test]
fn a_float_set_on_an_integer_boolean_or_unsigned_member_is_written_when_it_is_a_whole_number() {
    let int = |value| scalar_bytes(INT, PropertyValue::Float(value)).map(i32::from_le_bytes);
    assert_eq!(int(2.0), Ok(2));
    assert_eq!(int(-3.0), Ok(-3));
    assert_eq!(int(2.5), Err(wrong_kind()));
    assert_eq!(int(f32::NAN), Err(wrong_kind()));
    assert_eq!(int(3.0e9), Err(wrong_kind()), "past the largest integer");

    let flag = |value| scalar_bytes(BOOL, PropertyValue::Float(value)).map(u32::from_le_bytes);
    assert_eq!(flag(1.0), Ok(1));
    assert_eq!(flag(0.0), Ok(0));
    assert_eq!(flag(2.0), Err(wrong_kind()));

    let unsigned = |value| scalar_bytes(UINT, PropertyValue::Float(value)).map(u32::from_le_bytes);
    assert_eq!(unsigned(3.0), Ok(3));
    assert_eq!(unsigned(-1.0), Err(wrong_kind()));
}

#[test]
fn a_vector_on_a_scalar_member_stays_a_named_error() {
    assert_eq!(
        scalar_bytes(FLOAT, PropertyValue::Matrix([0.0; 16])),
        Err(wrong_kind())
    );
    assert_eq!(
        scalar_bytes(INT, PropertyValue::Vector([1.0; 4])),
        Err(wrong_kind())
    );
}

#[test]
fn the_first_layer_with_a_name_wins() {
    let cb = buffer(vec![param(1, 0, 1, FLOAT, 1)], vec![], 16);
    let (mut material, mut globals) = (PropertySet::new(), PropertySet::new());
    material.set("_Offset", PropertyValue::Float(1.0));
    globals.set("_Offset", PropertyValue::Float(5.0));
    let packed = pack_constant_buffer(&cb, &names(), &[&material, &globals], 0).unwrap();
    assert_eq!(floats(&packed.bytes)[..1], [1.0]);
    let packed = pack_constant_buffer(&cb, &names(), &[&PropertySet::new(), &globals], 0).unwrap();
    assert_eq!(
        floats(&packed.bytes)[..1],
        [5.0],
        "globals fill what the material lacks"
    );
}

#[test]
fn missing_properties_are_zero_and_reported_once() {
    let cb = buffer(
        vec![param(0, 0, 1, FLOAT, 4), param(1, 16, 1, FLOAT, 1)],
        vec![param(2, 32, 1, FLOAT, 4)],
        96,
    );
    let mut material = PropertySet::new();
    material.set("_Offset", PropertyValue::Float(3.0));
    let packed = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap();
    assert_eq!(
        packed.missing,
        vec!["_Tint".to_owned(), "_Matrix".to_owned()]
    );
    assert_eq!(floats(&packed.bytes)[0..4], [0.0; 4]);
    assert_eq!(floats(&packed.bytes)[4..5], [3.0]);
}

#[test]
fn the_buffer_is_at_least_the_shaders_size_rounded_to_sixteen_bytes() {
    let cb = buffer(vec![param(0, 0, 1, FLOAT, 4)], vec![], 16);
    assert_eq!(
        pack_constant_buffer(&cb, &names(), &[], 0)
            .unwrap()
            .bytes
            .len(),
        16
    );
    assert_eq!(
        pack_constant_buffer(&cb, &names(), &[], 40)
            .unwrap()
            .bytes
            .len(),
        48
    );
    let odd = buffer(vec![], vec![], 20);
    assert_eq!(
        pack_constant_buffer(&odd, &names(), &[], 0)
            .unwrap()
            .bytes
            .len(),
        32
    );
}

#[test]
fn a_negative_size_is_a_named_error() {
    let array = buffer(vec![param(0, 0, -1, FLOAT, 4)], vec![], 16);
    assert_eq!(
        pack_constant_buffer(&array, &names(), &[], 0).unwrap_err(),
        ConstantError::NegativeSize {
            name: "_Tint".to_owned(),
            field: "array size",
            value: -1
        }
    );
    let whole = buffer(vec![param(0, 0, 1, FLOAT, 4)], vec![], -16);
    assert!(matches!(
        pack_constant_buffer(&whole, &names(), &[], 0).unwrap_err(),
        ConstantError::NegativeSize {
            field: "size",
            value: -16,
            ..
        }
    ));
}

#[test]
fn bad_declarations_and_values_are_named_errors() {
    let tint = |value| {
        let mut material = PropertySet::new();
        material.set("_Tint", value);
        material
    };
    let cb = buffer(vec![param(0, 0, 1, FLOAT, 4)], vec![], 16);
    assert_eq!(
        pack_constant_buffer(&cb, &names(), &[&tint(PropertyValue::Float(1.0))], 0).unwrap_err(),
        ConstantError::WrongKind {
            property: "_Tint".to_owned()
        }
    );
    let unknown = buffer(vec![param(77, 0, 1, FLOAT, 4)], vec![], 16);
    assert_eq!(
        pack_constant_buffer(&unknown, &names(), &[], 0).unwrap_err(),
        ConstantError::UnknownName { name_index: 77 }
    );
    let outside = buffer(vec![param(0, 16, 1, FLOAT, 4)], vec![], 16);
    assert!(matches!(
        pack_constant_buffer(&outside, &names(), &[], 0).unwrap_err(),
        ConstantError::OutOfBounds { .. }
    ));
    let half = buffer(vec![param(6, 0, 1, HALF, 1)], vec![], 16);
    assert_eq!(
        pack_constant_buffer(&half, &names(), &[], 0).unwrap_err(),
        ConstantError::UnsupportedValue {
            name: "_Half".to_owned(),
            kind: i32::from(HALF)
        }
    );
    let wide = buffer(vec![param(0, 0, 1, FLOAT, 5)], vec![], 16);
    assert!(matches!(
        pack_constant_buffer(&wide, &names(), &[], 0).unwrap_err(),
        ConstantError::UnsupportedValue { .. }
    ));
}

#[test]
fn an_array_longer_than_its_declaration_is_a_named_error() {
    let cb = buffer(vec![param(3, 0, 1, FLOAT, 4)], vec![], 64);
    let mut material = PropertySet::new();
    material.set("_Weights", PropertyValue::VectorArray(vec![[0.0; 4]; 2]));
    let error = pack_constant_buffer(&cb, &names(), &[&material], 0).unwrap_err();
    assert_eq!(
        error,
        ConstantError::ArrayTooLong {
            property: "_Weights".to_owned(),
            declared: 1,
            given: 2
        }
    );
}

#[test]
fn a_struct_member_is_not_silently_skipped() {
    let mut cb = buffer(vec![], vec![], 16);
    cb.structs.push(StructParam {
        name_index: 0,
        index: 0,
        array_size: 1,
        struct_size: 16,
        vectors: Vec::new(),
        matrices: Vec::new(),
    });
    assert_eq!(
        pack_constant_buffer(&cb, &names(), &[], 0).unwrap_err(),
        ConstantError::UnsupportedStruct {
            name: "_Tint".to_owned()
        }
    );
}
