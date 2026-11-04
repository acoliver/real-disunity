//! The layout of `ComputeShader` objects: the kernels a compute shader declares, in order, with
//! the thread group size each one's first compiled variant states.
#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    compute_shader::{ComputeKernel, ComputeShader},
    write::ObjectWriter,
};

/// A constant buffer, texture or buffer binding: its name and five more 32-bit fields.
fn resource(w: &mut ObjectWriter, name: &str) {
    w.string(name).unwrap();
    for field in [0, 1, u32::MAX, u32::MAX, 0] {
        w.u32(field);
    }
}

fn resources(w: &mut ObjectWriter, names: &[&str]) {
    w.u32(u32::try_from(names.len()).unwrap());
    for name in names {
        resource(w, name);
    }
}

/// One compiled variant of a kernel: the bindings it declares, its code, the thread group size
/// it states and the bookkeeping that follows.
fn variant(w: &mut ObjectWriter, size: [u32; 3], samplers: u32) {
    resources(w, &["$Globals"]);
    resources(w, &["_Source", "_Other"]);
    w.u32(samplers);
    for _ in 0..samplers {
        w.u32(85).u32(0);
    }
    resources(w, &[]);
    resources(w, &["_Output"]);
    w.byte_array(&[0x44, 0x58, 0x42, 0x43, 1, 2, 3]).unwrap();
    w.u32(3).u32(size[0]).u32(size[1]).u32(size[2]);
    w.u32(50241).u32(0).u32(1);
    w.string("VRS_TILE_SIZE_8").unwrap();
}

fn kernel(w: &mut ObjectWriter, name: &str, variants: &[([u32; 3], u32)]) {
    w.string(name).unwrap();
    w.u32(u32::try_from(variants.len()).unwrap());
    w.u32(1).u32(7);
    for &(size, samplers) in variants {
        variant(w, size, samplers);
    }
    for _ in 0..4 {
        w.u32(0);
    }
}

/// A compute shader with one platform variant that holds `kernels`, followed by the constant
/// buffer list and a second platform variant the reader does not look at.
fn shader(name: &str, kernels: &[(&str, Vec<([u32; 3], u32)>)]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap();
    w.u32(2).u32(2).u32(0);
    w.u32(u32::try_from(kernels.len()).unwrap());
    for (kernel_name, variants) in kernels {
        kernel(&mut w, kernel_name, variants);
    }
    resources(&mut w, &["$Globals"]);
    w.u32(21).u32(0).u32(0).u32(0).u32(7);
    w.finish()
}

fn decoded(bytes: Vec<u8>) -> Result<ComputeShader, SerializedError> {
    decode_one::<ComputeShader>(ClassId::COMPUTE_SHADER, bytes, false)
}

fn kernel_of(name: &str, size: Option<[u32; 3]>) -> ComputeKernel {
    ComputeKernel {
        name: name.into(),
        thread_group_size: size,
    }
}

#[test]
fn reads_a_kernel_with_the_thread_group_size_its_variant_states() {
    let bytes = shader(
        "TerrainReadBackShader",
        &[("TerrainReadBackCS", vec![([8, 8, 1], 0)])],
    );
    let shader = decoded(bytes).unwrap();
    assert_eq!(shader.name, "TerrainReadBackShader");
    assert_eq!(
        shader.kernels,
        [kernel_of("TerrainReadBackCS", Some([8, 8, 1]))]
    );
}

#[test]
fn keeps_the_kernels_in_the_order_the_asset_lists_them() {
    let bytes = shader(
        "Histogram",
        &[
            ("KGather", vec![([16, 16, 1], 0)]),
            ("KClear", vec![([16, 1, 1], 0)]),
            ("KMerge", vec![([4, 4, 4], 0)]),
        ],
    );
    let kernels = decoded(bytes).unwrap().kernels;
    let seen: Vec<_> = kernels
        .iter()
        .map(|kernel| (kernel.name.as_str(), kernel.thread_group_size))
        .collect();
    assert_eq!(
        seen,
        [
            ("KGather", Some([16, 16, 1])),
            ("KClear", Some([16, 1, 1])),
            ("KMerge", Some([4, 4, 4])),
        ]
    );
}

#[test]
fn reads_past_every_variant_of_a_kernel_to_find_the_next_one() {
    let bytes = shader(
        "Variants",
        &[
            (
                "First",
                vec![([2, 2, 2], 0), ([9, 9, 9], 2), ([7, 7, 7], 1)],
            ),
            ("Second", vec![([32, 1, 1], 1)]),
        ],
    );
    let kernels = decoded(bytes).unwrap().kernels;
    assert_eq!(
        kernels,
        [
            kernel_of("First", Some([2, 2, 2])),
            kernel_of("Second", Some([32, 1, 1]))
        ]
    );
}

#[test]
fn a_kernel_that_states_no_size_has_none() {
    let bytes = shader("Unsized", &[("Main", vec![([0, 0, 0], 0)])]);
    assert_eq!(decoded(bytes).unwrap().kernels, [kernel_of("Main", None)]);
}

#[test]
fn a_shader_with_no_variants_has_no_kernels() {
    let mut w = ObjectWriter::new(false);
    w.string("Empty").unwrap();
    w.u32(0);
    assert_eq!(decoded(w.finish()).unwrap().kernels, []);
}

#[test]
fn truncated_data_is_an_error() {
    let bytes = shader("Cut", &[("Main", vec![([8, 8, 1], 0)])]);
    for length in [bytes.len() / 3, bytes.len() / 2, 40] {
        assert!(decoded(bytes[..length].to_vec()).is_err(), "{length}");
    }
}

#[test]
fn a_huge_kernel_count_is_refused_before_anything_is_allocated() {
    let mut w = ObjectWriter::new(false);
    w.string("Huge").unwrap();
    w.u32(2).u32(2).u32(0).u32(u32::MAX / 2);
    assert!(matches!(
        decoded(w.finish()),
        Err(SerializedError::InvalidCount { .. })
    ));
}

#[test]
fn a_thread_group_size_that_is_not_three_values_is_not_understood() {
    let mut w = ObjectWriter::new(false);
    w.string("Odd").unwrap();
    w.u32(2).u32(2).u32(0).u32(1);
    w.string("Main").unwrap();
    w.u32(1).u32(1).u32(0);
    resources(&mut w, &[]);
    resources(&mut w, &[]);
    w.u32(0);
    resources(&mut w, &[]);
    resources(&mut w, &[]);
    w.byte_array(&[]).unwrap();
    w.u32(2).u32(8).u32(8);
    w.u32(50241).u32(0).u32(0);
    for _ in 0..4 {
        w.u32(0);
    }
    let result = decoded(w.finish());
    assert!(
        matches!(result, Err(SerializedError::InvalidComputeShader { .. })),
        "{result:?}"
    );
}

#[test]
fn a_big_endian_shader_reads_the_same() {
    let mut w = ObjectWriter::new(true);
    w.string("Big").unwrap();
    w.u32(1).u32(2).u32(0).u32(1);
    w.string("Main").unwrap();
    w.u32(1).u32(1).u32(0);
    for _ in 0..2 {
        resources(&mut w, &[]);
    }
    w.u32(0);
    for _ in 0..2 {
        resources(&mut w, &[]);
    }
    w.byte_array(&[1]).unwrap();
    w.u32(3).u32(4).u32(2).u32(1);
    w.u32(50241).u32(0).u32(0);
    for _ in 0..4 {
        w.u32(0);
    }
    let shader = decode_one::<ComputeShader>(ClassId::COMPUTE_SHADER, w.finish(), true).unwrap();
    assert_eq!(shader.kernels, [kernel_of("Main", Some([4, 2, 1]))]);
}
