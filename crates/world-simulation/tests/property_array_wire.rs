//! The byte encoding of vector and matrix array properties: layout, round trips and refusals.

use worldrunner_world_simulation::graphics::snapshot_wire::{
    WIRE_VERSION, WireError, decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    ComputeBindings, ComputeShaderId, GlobalShaderState, GraphicsSnapshot, OffscreenWork,
    PropertyValue, RendererView, ShaderProperties,
};

const WORK_SET_GLOBAL: u8 = 6;
const TAG_VECTOR_ARRAY: u8 = 5;
const TAG_MATRIX_ARRAY: u8 = 6;
/// Where a global state with no keywords and no other values holds its two array lists in the
/// bytes of a snapshot: the header, three empty maps, the global marker, the empty keyword list
/// and five empty value lists.
const ARRAYS_AT: usize = 12 + 12 + 1 + 4 + 20;

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn text(name: &str) -> Vec<u8> {
    let mut bytes = u32::try_from(name.len()).unwrap().to_le_bytes().to_vec();
    bytes.extend_from_slice(name.as_bytes());
    bytes
}

/// One list entry: the name, the element count and the floats.
fn entry(name: &str, count: u32, values: &[f32]) -> Vec<u8> {
    let mut bytes = text(name);
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend(floats(values));
    bytes
}

/// One list: the number of entries, then the entries.
fn list(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = u32::try_from(entries.len()).unwrap().to_le_bytes().to_vec();
    for entry in entries {
        bytes.extend_from_slice(entry);
    }
    bytes
}

fn lists(vectors: &[Vec<u8>], matrices: &[Vec<u8>]) -> Vec<u8> {
    [list(vectors), list(matrices)].concat()
}

/// A snapshot with a global state holding `arrays` where its two array lists belong and nothing
/// else.
fn global_snapshot(arrays: &[u8]) -> Vec<u8> {
    let mut body = vec![0; 12];
    body.push(1);
    body.extend_from_slice(&[0; 4 + 20]);
    body.extend_from_slice(arrays);
    body.extend_from_slice(&0_u64.to_le_bytes());
    // Empty work, cameras, lights, renderers, compute shaders and compute buffers.
    body.extend_from_slice(&[0; 24]);
    let mut bytes = b"DGSN".to_vec();
    bytes.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    bytes.extend(body);
    bytes
}

fn vector(base: f32) -> [f32; 4] {
    [base, base + 1.0, base + 2.0, base + 3.0]
}

/// A matrix whose element at `column * 4 + row` is `base + 10 * column + row`, so a transposed
/// encoding changes every off-diagonal value.
fn matrix(base: f32) -> [f32; 16] {
    std::array::from_fn(|index| {
        let column = f32::from(u8::try_from(index / 4).unwrap());
        let row = f32::from(u8::try_from(index % 4).unwrap());
        base + 10.0 * column + row
    })
}

fn global_with(properties: ShaderProperties) -> GraphicsSnapshot {
    GraphicsSnapshot {
        global: Some(GlobalShaderState {
            keywords: Vec::new(),
            properties,
            generation: 0,
        }),
        ..GraphicsSnapshot::default()
    }
}

fn arrays_of(
    vectors: &[(&str, Vec<[f32; 4]>)],
    matrices: &[(&str, Vec<[f32; 16]>)],
) -> ShaderProperties {
    ShaderProperties {
        vector_arrays: vectors
            .iter()
            .map(|(name, values)| ((*name).to_owned(), values.clone()))
            .collect(),
        matrix_arrays: matrices
            .iter()
            .map(|(name, values)| ((*name).to_owned(), values.clone()))
            .collect(),
        ..ShaderProperties::default()
    }
}

#[test]
fn the_version_is_seven_and_versions_four_to_six_are_refused() {
    assert_eq!(WIRE_VERSION, 7);
    for old in [4_u32, 5, 6] {
        let mut bytes = encode(&GraphicsSnapshot::default()).unwrap();
        bytes[4..8].copy_from_slice(&old.to_le_bytes());
        assert_eq!(decode(&bytes), Err(WireError::UnknownVersion(old)));
    }
}

#[test]
fn array_lists_follow_the_five_older_lists_in_a_fixed_layout() {
    let vectors = vec![("_V", vec![vector(0.0), vector(4.0)])];
    let matrices = vec![("_M", vec![matrix(1.0)])];
    let snapshot = global_with(arrays_of(&vectors, &matrices));
    let expected = lists(
        &[entry("_V", 2, &[vector(0.0), vector(4.0)].concat())],
        &[entry("_M", 1, &matrix(1.0))],
    );
    let bytes = encode(&snapshot).unwrap();
    assert_eq!(&bytes[ARRAYS_AT..ARRAYS_AT + expected.len()], expected);
    assert_eq!(bytes, global_snapshot(&expected));
    assert_eq!(decode(&bytes).unwrap(), snapshot);
}

#[test]
fn a_matrix_keeps_its_column_major_element_order_and_every_float_bit() {
    let mut odd = matrix(0.0);
    odd[0] = -0.0;
    odd[5] = f32::from_bits(1);
    odd[15] = f32::MAX;
    let values = vec![[-0.0, f32::MIN_POSITIVE, 1.0 / 3.0, -3.0e30]];
    let snapshot = global_with(arrays_of(
        &[("_V", values.clone())],
        &[("_M", vec![odd, matrix(2.0)])],
    ));
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    let properties = back.global.unwrap().properties;
    let held = properties.matrix_array("_M").unwrap();
    assert_eq!(held[0].map(f32::to_bits), odd.map(f32::to_bits));
    assert_eq!(held[1].map(f32::to_bits), matrix(2.0).map(f32::to_bits));
    assert_eq!(
        held[1][4].to_bits(),
        12.0_f32.to_bits(),
        "element 4 is column one, row zero"
    );
    let vectors = properties.vector_array("_V").unwrap();
    assert_eq!(vectors[0].map(f32::to_bits), values[0].map(f32::to_bits));
}

#[test]
fn legacy_scalar_values_keep_nan_payloads_next_to_arrays() {
    let mut properties = arrays_of(&[("_V", vec![vector(1.0)])], &[]);
    properties
        .floats
        .push(("_F".into(), f32::from_bits(0x7FC0_1234)));
    properties
        .vectors
        .push(("_Vec".into(), [f32::NAN, f32::INFINITY, -0.0, 1.0]));
    let back = decode(&encode(&global_with(properties)).unwrap()).unwrap();
    let properties = back.global.unwrap().properties;
    assert_eq!(properties.floats[0].1.to_bits(), 0x7FC0_1234);
    assert!(properties.vectors[0].1[0].is_nan());
    assert_eq!(properties.vector_array("_V"), Some(vec![vector(1.0)]));
}

fn every_location() -> GraphicsSnapshot {
    let properties = arrays_of(&[("_V", vec![vector(3.0)])], &[("_M", vec![matrix(7.0)])]);
    let mut snapshot = global_with(properties.clone());
    let material = worldrunner_world_simulation::graphics::WorldMaterial {
        shader: worldrunner_world_simulation::graphics::ShaderId::from_bits(1 << 32 | 1),
        render_queue: -1,
        keywords: Vec::new(),
        disabled_passes: Vec::new(),
        properties: properties.clone(),
        generation: 1,
    };
    snapshot.materials.insert(
        worldrunner_world_simulation::graphics::MaterialId::from_bits(1 << 32 | 1),
        material,
    );
    snapshot.renderers.push(RendererView {
        local_to_world: [0.0; 16],
        mesh: 5,
        layer: 0,
        materials: vec![None],
        render_queue: None,
        property_block: Some(properties.clone()),
    });
    snapshot.work = vec![
        OffscreenWork::SetGlobal(PropertyValue::VectorArray(
            "_Ordered".into(),
            vec![vector(9.0)],
        )),
        OffscreenWork::SetGlobal(PropertyValue::MatrixArray(
            "_Ordered".into(),
            vec![matrix(1.0), matrix(2.0)],
        )),
        OffscreenWork::Dispatch {
            shader: ComputeShaderId::from_bits(1 << 32 | 1),
            kernel: 0,
            groups: [1, 1, 1],
            bindings: ComputeBindings {
                properties,
                buffers: Vec::new(),
            },
        },
    ];
    snapshot
}

#[test]
fn arrays_round_trip_in_globals_materials_blocks_compute_bindings_and_ordered_work() {
    let snapshot = every_location();
    assert_eq!(decode(&encode(&snapshot).unwrap()).unwrap(), snapshot);
}

#[test]
fn a_merged_set_may_hold_more_than_1024_array_names() {
    let names: Vec<String> = (0..1500).map(|index| format!("_n{index:05}")).collect();
    let vectors: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), vec![vector(1.0)]))
        .collect();
    let matrices: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), vec![matrix(1.0)]))
        .collect();
    let snapshot = global_with(arrays_of(&vectors, &matrices));
    assert_eq!(decode(&encode(&snapshot).unwrap()).unwrap(), snapshot);
}

fn work_bytes(tag: u8, name: &str, count: u32, values: &[f32]) -> Vec<u8> {
    let mut bytes = vec![WORK_SET_GLOBAL, tag];
    bytes.extend(entry(name, count, values));
    bytes
}

#[test]
fn recorded_array_work_has_a_fixed_layout_and_round_trips() {
    let vectors = vec![vector(0.0), vector(8.0), vector(16.0)];
    let work = OffscreenWork::SetGlobal(PropertyValue::VectorArray("_Pts".into(), vectors.clone()));
    let expected = work_bytes(TAG_VECTOR_ARRAY, "_Pts", 3, &vectors.concat());
    assert_eq!(encode_work(&work).unwrap(), expected);
    assert_eq!(decode_work(&expected).unwrap(), work);

    let matrices = vec![matrix(0.0), matrix(100.0)];
    let work =
        OffscreenWork::SetGlobal(PropertyValue::MatrixArray("_Mats".into(), matrices.clone()));
    let expected = work_bytes(TAG_MATRIX_ARRAY, "_Mats", 2, &matrices.concat());
    assert_eq!(encode_work(&work).unwrap(), expected);
    assert_eq!(decode_work(&expected).unwrap(), work);
}

#[test]
fn a_property_tag_past_the_matrix_array_is_unknown() {
    let bytes = [WORK_SET_GLOBAL, 7];
    assert_eq!(decode_work(&bytes), Err(WireError::UnknownTag(7)));
}

#[test]
fn array_work_cut_short_at_any_byte_is_truncated_and_a_trailing_byte_is_refused() {
    let vectors = work_bytes(
        TAG_VECTOR_ARRAY,
        "_Pts",
        2,
        &[vector(0.0), vector(1.0)].concat(),
    );
    let matrices = work_bytes(TAG_MATRIX_ARRAY, "_M", 1, &matrix(0.0));
    for bytes in [vectors, matrices] {
        decode_work(&bytes).unwrap();
        for length in 0..bytes.len() {
            assert_eq!(
                decode_work(&bytes[..length]),
                Err(WireError::Truncated),
                "cut at {length}"
            );
        }
        let mut extra = bytes;
        extra.push(0);
        assert_eq!(decode_work(&extra), Err(WireError::TrailingBytes));
    }
}

#[test]
fn array_work_with_a_bad_count_is_refused_before_any_payload_is_read() {
    let floats_for = |count: usize| vec![1.0; count * 4];
    let cases = [
        (0, floats_for(0), WireError::InvalidPropertyArray),
        (1024, floats_for(1024), WireError::TooLong),
        (u32::MAX, floats_for(1), WireError::TooLong),
        (1023, floats_for(1), WireError::Truncated),
        (1 << 30, Vec::new(), WireError::TooLong),
    ];
    for (count, values, error) in cases {
        let bytes = work_bytes(TAG_VECTOR_ARRAY, "_V", count, &values);
        assert_eq!(decode_work(&bytes), Err(error), "count {count}");
    }
    let bytes = work_bytes(TAG_MATRIX_ARRAY, "_M", 0, &[]);
    assert_eq!(decode_work(&bytes), Err(WireError::InvalidPropertyArray));
    let bytes = work_bytes(TAG_MATRIX_ARRAY, "_M", 1023, &[1.0; 16]);
    assert_eq!(decode_work(&bytes), Err(WireError::Truncated));
    let bytes = work_bytes(TAG_MATRIX_ARRAY, "_M", u32::MAX, &[]);
    assert_eq!(decode_work(&bytes), Err(WireError::TooLong));
}

#[test]
fn array_work_with_a_non_finite_component_is_refused_wherever_it_sits() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for slot in 0..8 {
            let mut values = [vector(0.0), vector(4.0)].concat();
            values[slot] = bad;
            let bytes = work_bytes(TAG_VECTOR_ARRAY, "_V", 2, &values);
            assert_eq!(decode_work(&bytes), Err(WireError::InvalidPropertyArray));
        }
        let mut values = [matrix(0.0), matrix(1.0)].concat();
        values[31] = bad;
        let bytes = work_bytes(TAG_MATRIX_ARRAY, "_M", 2, &values);
        assert_eq!(decode_work(&bytes), Err(WireError::InvalidPropertyArray));
    }
}

#[test]
fn array_work_names_must_be_text_of_one_to_256_bytes() {
    let one = [1.0; 4];
    let mut bad_text = work_bytes(TAG_VECTOR_ARRAY, "AB", 1, &one);
    let at = bad_text.windows(2).position(|pair| pair == b"AB").unwrap();
    bad_text[at] = 0xFF;
    assert_eq!(decode_work(&bad_text), Err(WireError::BadText));
    let empty = work_bytes(TAG_VECTOR_ARRAY, "", 1, &one);
    assert_eq!(decode_work(&empty), Err(WireError::InvalidPropertyArray));
    let longest = work_bytes(TAG_VECTOR_ARRAY, &"n".repeat(256), 1, &one);
    decode_work(&longest).unwrap();
    let too_long = work_bytes(TAG_VECTOR_ARRAY, &"n".repeat(257), 1, &one);
    assert_eq!(decode_work(&too_long), Err(WireError::TooLong));
}

#[test]
fn the_encoder_refuses_what_the_decoder_refuses() {
    let empty = PropertyValue::VectorArray("_V".into(), Vec::new());
    let result = encode_work(&OffscreenWork::SetGlobal(empty));
    assert_eq!(result, Err(WireError::InvalidPropertyArray));
    let long = PropertyValue::MatrixArray("_M".into(), vec![[0.0; 16]; 1024]);
    assert_eq!(
        encode_work(&OffscreenWork::SetGlobal(long)),
        Err(WireError::TooLong)
    );
    let mut values = vec![vector(0.0)];
    values[0][3] = f32::NAN;
    let result = encode_work(&OffscreenWork::SetGlobal(PropertyValue::VectorArray(
        "_V".into(),
        values,
    )));
    assert_eq!(result, Err(WireError::InvalidPropertyArray));
    let nameless = PropertyValue::MatrixArray(String::new(), vec![[0.0; 16]]);
    assert_eq!(
        encode_work(&OffscreenWork::SetGlobal(nameless)),
        Err(WireError::InvalidPropertyArray)
    );

    let unsorted = arrays_of(&[("_b", vec![vector(0.0)]), ("_a", vec![vector(0.0)])], &[]);
    assert_eq!(
        encode(&global_with(unsorted)),
        Err(WireError::InvalidPropertyArray)
    );
    let twice = arrays_of(&[], &[("_a", vec![matrix(0.0)]), ("_a", vec![matrix(0.0)])]);
    assert_eq!(
        encode(&global_with(twice)),
        Err(WireError::InvalidPropertyArray)
    );
}

#[test]
fn snapshot_array_lists_must_be_strictly_ascending_by_name() {
    let one = |name: &str| entry(name, 1, &vector(0.0));
    let ascending = global_snapshot(&lists(&[one("_a"), one("_b")], &[]));
    decode(&ascending).unwrap();
    for names in [["_b", "_a"], ["_a", "_a"]] {
        let bytes = global_snapshot(&lists(&[one(names[0]), one(names[1])], &[]));
        assert_eq!(decode(&bytes), Err(WireError::InvalidPropertyArray));
    }
    let matrix_entry = |name: &str| entry(name, 1, &matrix(0.0));
    let bytes = global_snapshot(&lists(&[], &[matrix_entry("_z"), matrix_entry("_y")]));
    assert_eq!(decode(&bytes), Err(WireError::InvalidPropertyArray));
}

#[test]
fn snapshot_array_entries_with_bad_names_counts_or_floats_are_refused() {
    let vector_entry = |name: &str, count: u32, values: &[f32]| entry(name, count, values);
    let cases = [
        (
            vector_entry("", 1, &[1.0; 4]),
            WireError::InvalidPropertyArray,
        ),
        (
            vector_entry(&"n".repeat(257), 1, &[1.0; 4]),
            WireError::TooLong,
        ),
        (vector_entry("_v", 0, &[]), WireError::InvalidPropertyArray),
        (vector_entry("_v", 1024, &[]), WireError::TooLong),
        (vector_entry("_v", 2, &[1.0; 4]), WireError::Truncated),
        (
            vector_entry("_v", 1, &[0.0, f32::NAN, 0.0, 0.0]),
            WireError::InvalidPropertyArray,
        ),
    ];
    for (bad, error) in cases {
        let bytes = global_snapshot(&lists(&[bad], &[]));
        assert_eq!(decode(&bytes), Err(error));
    }
    let mut text_bytes = entry("AB", 1, &[1.0; 4]);
    text_bytes[4] = 0xFF;
    let bytes = global_snapshot(&lists(&[text_bytes], &[]));
    assert_eq!(decode(&bytes), Err(WireError::BadText));
}

#[test]
fn a_list_count_over_the_cap_or_larger_than_its_bytes_could_hold_is_refused() {
    let counts_only =
        |vectors: u32, matrices: u32| [vectors.to_le_bytes(), matrices.to_le_bytes()].concat();
    let bytes = global_snapshot(&counts_only(u32::MAX, 0));
    assert_eq!(decode(&bytes), Err(WireError::TooLong));
    let bytes = global_snapshot(&counts_only((1 << 22) + 1, 0));
    assert_eq!(decode(&bytes), Err(WireError::TooLong));
    let bytes = global_snapshot(&counts_only(1 << 22, 0));
    assert_eq!(decode(&bytes), Err(WireError::Truncated));
    let bytes = global_snapshot(&counts_only(0, 1 << 22));
    assert_eq!(decode(&bytes), Err(WireError::Truncated));
    let bytes = global_snapshot(&counts_only(2, 0));
    assert_eq!(decode(&bytes), Err(WireError::Truncated));
}

#[test]
fn a_snapshot_with_arrays_cut_short_at_any_byte_is_truncated() {
    let bytes = encode(&every_location()).unwrap();
    for length in 0..bytes.len() {
        assert!(
            decode(&bytes[..length]).is_err(),
            "a snapshot cut at {length} decoded"
        );
    }
    let first = decode(&bytes[..bytes.len() - 1]);
    assert_eq!(first, Err(WireError::Truncated));
}

#[test]
fn bytes_after_a_snapshot_with_arrays_are_refused() {
    let mut bytes = encode(&every_location()).unwrap();
    bytes.push(0);
    assert_eq!(decode(&bytes), Err(WireError::TrailingBytes));
}
