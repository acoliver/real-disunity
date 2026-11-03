//! Constant buffers of a player variant that hold a struct. In the player measured, a buffer with
//! structs is one array of structs that starts at byte 0 and has no members of its own; an
//! element's members sit at offsets from the element's start, the elements are the struct's size
//! apart, and a member is named by its own name.

use worldrunner_render_program::constants::{ConstantError, PackedConstants, pack_variant_buffer};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_unity_serialized::shader_parameters::{BufferMember, ConstantBuffer, StructLayout};

const FLOAT: i32 = 0;
const INT: i32 = 1;

/// Bytes of one element in the buffers below: a float at 0, a float4 at 16 and an int4 at 32.
const ELEMENT: usize = 48;

fn member(name: &str, kind: i32, shape: (i32, i32), offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind,
        rows: shape.0,
        columns: shape.1,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

fn matrix(name: &str, offset: i32) -> BufferMember {
    BufferMember {
        is_matrix: true,
        ..member(name, FLOAT, (4, 4), offset)
    }
}

fn item_members() -> Vec<BufferMember> {
    vec![
        member("_Rate", FLOAT, (1, 1), 0),
        member("_Pos", FLOAT, (1, 4), 16),
        member("_Mask", INT, (1, 4), 32),
    ]
}

fn item(array_size: i32, size: i32, members: Vec<BufferMember>) -> StructLayout {
    StructLayout {
        name: "Item".to_owned(),
        index: 0,
        array_size,
        size,
        members,
    }
}

/// A buffer of nothing but one struct array, as the player has it.
fn buffer_of(structs: Vec<StructLayout>, size: i32) -> ConstantBuffer {
    ConstantBuffer {
        name: "Items".to_owned(),
        size,
        members: Vec::new(),
        structs,
    }
}

fn items(count: i32) -> ConstantBuffer {
    buffer_of(
        vec![item(count, 48, item_members())],
        count.max(1) * i32::try_from(ELEMENT).unwrap(),
    )
}

fn properties(values: Vec<(&str, PropertyValue)>) -> PropertySet {
    let mut set = PropertySet::new();
    for (name, value) in values {
        set.set(name, value);
    }
    set
}

fn pack(buffer: &ConstantBuffer, set: &PropertySet) -> Result<PackedConstants, ConstantError> {
    pack_variant_buffer(buffer, &[set], 0)
}

/// Writes `values` as little-endian floats at `at`.
fn put_floats(bytes: &mut [u8], at: usize, values: &[f32]) {
    for (i, value) in values.iter().enumerate() {
        bytes[at + 4 * i..at + 4 * i + 4].copy_from_slice(&value.to_le_bytes());
    }
}

fn put_ints(bytes: &mut [u8], at: usize, values: &[i32]) {
    for (i, value) in values.iter().enumerate() {
        bytes[at + 4 * i..at + 4 * i + 4].copy_from_slice(&value.to_le_bytes());
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn a_buffer_with_one_struct_lands_each_member_at_its_offset_from_the_structs_start() {
    // A struct that is not an array declares an array size of 0 or 1.
    for array_size in [0, 1] {
        let set = properties(vec![
            ("_Rate", PropertyValue::Float(2.5)),
            ("_Pos", PropertyValue::Vector([1.0, 2.0, 3.0, 4.0])),
            ("_Mask", PropertyValue::IntVector([5, 6, 7, 8])),
        ]);
        let packed = pack(&items(array_size), &set).unwrap();
        let mut want = vec![0; ELEMENT];
        put_floats(&mut want, 0, &[2.5]);
        put_floats(&mut want, 16, &[1.0, 2.0, 3.0, 4.0]);
        put_ints(&mut want, 32, &[5, 6, 7, 8]);
        assert_eq!(packed.bytes, want, "array size {array_size}");
        assert!(packed.missing.is_empty());
    }
}

#[test]
fn an_array_of_structs_puts_element_e_of_each_array_property_at_e_times_the_structs_size() {
    let set = properties(vec![
        ("_Rate", PropertyValue::Float(2.5)),
        (
            "_Pos",
            PropertyValue::VectorArray(vec![[1.0; 4], [2.0; 4], [3.0; 4]]),
        ),
        (
            "_Mask",
            PropertyValue::IntVectorArray(vec![[10, 11, 12, 13], [20, 21, 22, 23]]),
        ),
    ]);
    let packed = pack(&items(3), &set).unwrap();
    assert_eq!(packed.bytes.len(), 3 * ELEMENT);
    let mut want = vec![0; 3 * ELEMENT];
    // A single value is element 0's; the other elements keep zero.
    put_floats(&mut want, 0, &[2.5]);
    for (e, value) in [1.0, 2.0, 3.0].into_iter().enumerate() {
        put_floats(&mut want, e * ELEMENT + 16, &[value; 4]);
    }
    put_ints(&mut want, 32, &[10, 11, 12, 13]);
    put_ints(&mut want, ELEMENT + 32, &[20, 21, 22, 23]);
    assert_eq!(packed.bytes, want);
    assert!(packed.missing.is_empty());
}

#[test]
fn a_matrix_member_writes_four_rows_a_register_apart_in_every_element() {
    let members = vec![matrix("_Basis", 0), member("_Pos", FLOAT, (1, 4), 64)];
    let buffer = buffer_of(vec![item(2, 80, members)], 160);
    let sequence = |first: f32| -> [f32; 16] { std::array::from_fn(|i| first + i as f32) };
    let set = properties(vec![
        (
            "_Basis",
            PropertyValue::MatrixArray(vec![sequence(0.0), sequence(100.0)]),
        ),
        ("_Pos", PropertyValue::VectorArray(vec![[7.0; 4], [8.0; 4]])),
    ]);
    let got = floats(&pack(&buffer, &set).unwrap().bytes);
    assert_eq!(got.len(), 40);
    assert_eq!(got[0..16], sequence(0.0));
    assert_eq!(got[16..20], [7.0; 4]);
    assert_eq!(got[20..36], sequence(100.0));
    assert_eq!(got[36..40], [8.0; 4]);
}

#[test]
fn members_listed_out_of_offset_order_still_land_at_their_own_offsets() {
    let mut reversed = item_members();
    reversed.reverse();
    let buffer = buffer_of(vec![item(2, 48, reversed)], 96);
    let set = properties(vec![
        ("_Pos", PropertyValue::VectorArray(vec![[1.0; 4], [2.0; 4]])),
        (
            "_Mask",
            PropertyValue::IntVectorArray(vec![[1, 2, 3, 4], [5, 6, 7, 8]]),
        ),
    ]);
    let forward = pack(&items(2), &set).unwrap();
    assert_eq!(pack(&buffer, &set).unwrap(), forward);
    assert_eq!(forward.missing, ["_Rate"]);
}

#[test]
fn a_member_is_named_by_its_own_name_not_by_the_structs_name_or_a_path() {
    let set = properties(vec![
        ("Item_Pos", PropertyValue::Vector([1.0; 4])),
        ("Item._Pos", PropertyValue::Vector([2.0; 4])),
        ("Item", PropertyValue::Vector([3.0; 4])),
    ]);
    let packed = pack(&items(2), &set).unwrap();
    assert!(packed.bytes.iter().all(|&b| b == 0));
    assert_eq!(packed.missing, ["_Rate", "_Pos", "_Mask"]);
}

#[test]
fn a_missing_property_stays_zero_and_is_reported_once_for_all_elements() {
    let set = properties(vec![(
        "_Rate",
        PropertyValue::VectorArray(vec![[1.0, 0.0, 0.0, 0.0]; 4]),
    )]);
    let packed = pack(&items(4), &set).unwrap();
    assert_eq!(packed.missing, ["_Pos", "_Mask"]);
    for e in 0..4 {
        assert_eq!(floats(&packed.bytes[e * ELEMENT..][..4]), [1.0]);
        assert!(
            packed.bytes[e * ELEMENT + 16..(e + 1) * ELEMENT]
                .iter()
                .all(|&b| b == 0)
        );
    }
}

#[test]
fn the_buffer_holds_at_least_the_shaders_size_rounded_to_sixteen_bytes() {
    let set = PropertySet::new();
    let packed = pack_variant_buffer(&items(2), &[&set], 100).unwrap();
    assert_eq!(packed.bytes.len(), 112);
}

fn refused(buffer: &ConstantBuffer) -> ConstantError {
    pack(buffer, &properties(vec![])).unwrap_err()
}

#[test]
fn shapes_the_player_checked_does_not_have_are_named_errors_not_guesses() {
    let named = |name: &str| ConstantError::UnsupportedStruct {
        name: name.to_owned(),
    };
    // A struct that does not start at byte 0.
    let mut moved = item(2, 48, item_members());
    moved.index = 16;
    assert_eq!(refused(&buffer_of(vec![moved], 128)), named("Item"));
    // Two structs in one buffer.
    let second = StructLayout {
        name: "Other".to_owned(),
        ..item(2, 48, item_members())
    };
    let two = buffer_of(vec![item(2, 48, item_members()), second], 192);
    assert_eq!(refused(&two), named("Other"));
    // A struct beside the buffer's own members.
    let mut mixed = items(2);
    mixed.members.push(member("_Top", FLOAT, (1, 4), 96));
    mixed.size = 112;
    assert_eq!(refused(&mixed), named("Item"));
}

#[test]
fn sizes_and_members_that_do_not_fit_are_named_errors() {
    assert_eq!(
        refused(&buffer_of(vec![item(-1, 48, item_members())], 48)),
        ConstantError::NegativeSize {
            name: "Item".to_owned(),
            field: "struct array size",
            value: -1
        }
    );
    assert_eq!(
        refused(&buffer_of(vec![item(2, -48, item_members())], 96)),
        ConstantError::NegativeSize {
            name: "Item".to_owned(),
            field: "struct size",
            value: -48
        }
    );
    // The float4 at 40 would end at 56, past a struct of 48 bytes.
    let members = vec![member("_Pos", FLOAT, (1, 4), 40)];
    assert_eq!(
        refused(&buffer_of(vec![item(2, 48, members)], 96)),
        ConstantError::MemberOutsideStruct {
            name: "_Pos".to_owned(),
            offset: 40,
            size: 16,
            struct_size: 48
        }
    );
    // Four elements of 48 bytes need 148 bytes to hold the last member of the first member.
    assert_eq!(
        refused(&buffer_of(vec![item(4, 48, item_members())], 100)),
        ConstantError::OutOfBounds {
            name: "_Rate".to_owned(),
            offset: 0,
            size: 148,
            buffer: 112
        }
    );
}

#[test]
fn member_shapes_the_packer_does_not_write_in_a_struct_are_named_errors() {
    let in_struct = |member: BufferMember| refused(&buffer_of(vec![item(2, 48, vec![member])], 96));
    let unsupported = |name: &str, kind: i32| ConstantError::UnsupportedValue {
        name: name.to_owned(),
        kind,
    };
    let array = BufferMember {
        array_size: 2,
        ..member("_Many", FLOAT, (1, 4), 0)
    };
    assert_eq!(in_struct(array), unsupported("_Many", 0));
    assert_eq!(
        in_struct(member("_Ints", INT, (1, 3), 0)),
        unsupported("_Ints", 1)
    );
    assert_eq!(
        in_struct(member("_Flag", 2, (1, 1), 0)),
        unsupported("_Flag", 2)
    );
    assert_eq!(
        in_struct(member("_Wide", FLOAT, (1, 5), 0)),
        unsupported("_Wide", 0)
    );
}

#[test]
fn values_of_the_wrong_kind_or_too_many_elements_are_named_errors() {
    let wrong = |name: &str, value| pack(&items(3), &properties(vec![(name, value)])).unwrap_err();
    assert_eq!(
        wrong("_Mask", PropertyValue::Vector([1.0; 4])),
        ConstantError::WrongKind {
            property: "_Mask".to_owned()
        }
    );
    assert_eq!(
        wrong("_Pos", PropertyValue::IntVector([1; 4])),
        ConstantError::WrongKind {
            property: "_Pos".to_owned()
        }
    );
    assert_eq!(
        wrong("_Pos", PropertyValue::VectorArray(vec![[0.0; 4]; 4])),
        ConstantError::ArrayTooLong {
            property: "_Pos".to_owned(),
            declared: 3,
            given: 4
        }
    );
    assert_eq!(
        wrong("_Mask", PropertyValue::IntVectorArray(vec![[0; 4]; 4])),
        ConstantError::ArrayTooLong {
            property: "_Mask".to_owned(),
            declared: 3,
            given: 4
        }
    );
}
