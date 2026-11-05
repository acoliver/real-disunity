//! A compute kernel with Unity-style sparse bindings, run on the GPU and read back.

use worldrunner_render_program::constants::pack_constant_buffer;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::StageKind;
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{
    BoundResource, ComputeProgram, DispatchRequest, ProgramRuntime,
};
use worldrunner_unity_serialized::shader_program::{
    ConstantBuffer, ProgramParameters, UavParam, ValueParam,
};

use crate::common::{binding, run, runtime, same, stage, submit, words_to_bytes};

const KERNEL: &str = "
struct Params { scale: u32, bias: u32, count: u32 }
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read> source: array<u32>;
@group(0) @binding(7) var<storage, read_write> result: array<u32>;
@compute @workgroup_size(4) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x < params.count) {
        result[id.x] = source[id.x] * params.scale + params.bias + id.x;
    }
}";

const INT: i8 = 1;
const ELEMENTS: usize = 10;
const SENTINEL: u32 = 0xdead_beef;

fn int_param(name_index: i32, offset: i32) -> ValueParam {
    ValueParam {
        name_index,
        index: offset,
        array_size: 1,
        kind: INT,
        dimension: 1,
    }
}

fn parameters() -> ProgramParameters {
    ProgramParameters {
        constant_buffer_bindings: vec![binding(3)],
        constant_buffers: vec![ConstantBuffer {
            name_index: 3,
            matrices: vec![],
            vectors: vec![int_param(0, 0), int_param(1, 4), int_param(2, 8)],
            structs: vec![],
            size: 16,
            partial: false,
        }],
        buffers: vec![binding(5)],
        uavs: vec![UavParam {
            name_index: 7,
            index: 7,
            original_index: 7,
        }],
        ..ProgramParameters::default()
    }
}

#[test]
fn a_kernel_writes_the_storage_buffer_and_only_within_its_count() {
    let table = vec![same(0, 3), same(0, 5), same(0, 7)];
    let kernel = stage(KERNEL, ("main", StageKind::Compute), table, parameters());
    let mut runtime = runtime();
    let program = runtime.compute_program(&kernel).unwrap();

    let names: Vec<(String, i32)> = ["_Scale", "_Bias", "_Count", "Params"]
        .iter()
        .zip(0..)
        .map(|(name, index)| ((*name).to_owned(), index))
        .collect();
    let mut material = PropertySet::new();
    material.set("_Scale", PropertyValue::Int(3));
    material.set("_Bias", PropertyValue::Int(100));
    material.set(
        "_Count",
        PropertyValue::Int(i32::try_from(ELEMENTS).unwrap()),
    );
    let packed = pack_constant_buffer(
        &parameters().constant_buffers[0],
        &names,
        &[&material],
        usize::try_from(program.layout().slot(0, 3).unwrap().min_size).unwrap(),
    )
    .unwrap();
    assert!(packed.missing.is_empty());

    let source: Vec<u32> = (0..12).map(|i| 7 * i + 1).collect();
    let uniform = runtime.create_buffer("params", &packed.bytes).unwrap();
    let input = runtime
        .create_buffer("source", &words_to_bytes(&source))
        .unwrap();
    let output = runtime
        .create_buffer("result", &words_to_bytes(&[SENTINEL; 12]))
        .unwrap();
    run(&mut runtime, |runtime, encoder| {
        runtime.dispatch(
            encoder,
            &DispatchRequest {
                program: &program,
                bindings: &[
                    BoundResource::buffer(0, 3, uniform),
                    BoundResource::buffer(0, 5, input),
                    BoundResource::buffer(0, 7, output),
                ],
                groups: [3, 1, 1],
            },
        )
    })
    .unwrap();

    let (pending, index) = submit(&mut runtime, |runtime, encoder| {
        runtime.read_buffer(encoder, output)
    })
    .unwrap();
    let bytes = pending.resolve(index).unwrap();
    let got: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    // Closed form: source[i] = 7i + 1, so result[i] = 3 (7i + 1) + 100 + i = 22i + 103 for the
    // first ten elements; invocations 10 and 11 run in the third group and must write nothing.
    let expected: Vec<u32> = (0..12)
        .map(|i| if i < 10 { 22 * i + 103 } else { SENTINEL })
        .collect();
    assert_eq!(got, expected);
}

pub fn kernel_program(
    runtime: &worldrunner_render_program::runtime::ProgramRuntime,
) -> worldrunner_render_program::runtime::ComputeProgram {
    let table = vec![same(0, 3), same(0, 5), same(0, 7)];
    let kernel = stage(KERNEL, ("main", StageKind::Compute), table, parameters());
    runtime.compute_program(&kernel).unwrap()
}

fn failure(
    runtime: &mut ProgramRuntime,
    program: &ComputeProgram,
    bindings: &[BoundResource],
) -> ProgramError {
    run(runtime, |runtime, encoder| {
        runtime.dispatch(
            encoder,
            &DispatchRequest {
                program,
                bindings,
                groups: [1, 1, 1],
            },
        )
    })
    .unwrap_err()
}

#[test]
fn a_binding_set_that_differs_from_the_programs_is_a_named_error() {
    let mut runtime = runtime();
    let program = kernel_program(&runtime);
    let big = runtime.create_buffer("big", &[0; 64]).unwrap();
    let complete = [
        BoundResource::buffer(0, 3, big),
        BoundResource::buffer(0, 5, big),
        BoundResource::buffer(0, 7, big),
    ];
    assert_eq!(
        failure(&mut runtime, &program, &complete[..2]),
        ProgramError::MissingBinding { set: 0, binding: 7 }
    );
    let extra = [
        complete[0],
        complete[1],
        complete[2],
        BoundResource::buffer(0, 9, big),
    ];
    assert_eq!(
        failure(&mut runtime, &program, &extra),
        ProgramError::UnexpectedBinding { set: 0, binding: 9 }
    );
    let twice = [complete[0], complete[0], complete[1], complete[2]];
    assert_eq!(
        failure(&mut runtime, &program, &twice),
        ProgramError::UnexpectedBinding { set: 0, binding: 3 }
    );
    assert!(runtime.release_buffer(big).unwrap());
    assert_eq!(
        failure(&mut runtime, &program, &complete),
        ProgramError::UnknownHandle { kind: "buffer" }
    );
}

#[test]
fn a_resource_that_does_not_suit_its_binding_is_a_named_error() {
    let mut runtime = runtime();
    let program = kernel_program(&runtime);
    let big = runtime.create_buffer("big", &[0; 64]).unwrap();
    let small = runtime.create_buffer("small", &[0; 4]).unwrap();
    let texture = runtime
        .create_texture_2d("t", [1, 1], wgpu::TextureFormat::Rgba8Unorm, &[0; 4])
        .unwrap();
    let rest = [
        BoundResource::buffer(0, 5, big),
        BoundResource::buffer(0, 7, big),
    ];
    let short = [BoundResource::buffer(0, 3, small), rest[0], rest[1]];
    assert_eq!(
        failure(&mut runtime, &program, &short),
        ProgramError::BufferTooSmall {
            set: 0,
            binding: 3,
            needed: 12,
            have: 4
        }
    );
    let wrong = [BoundResource::texture(0, 3, texture), rest[0], rest[1]];
    assert!(matches!(
        failure(&mut runtime, &program, &wrong),
        ProgramError::WrongResource {
            set: 0,
            binding: 3,
            ..
        }
    ));
}

#[test]
fn equal_dispatches_share_one_pipeline_and_one_module() {
    let mut runtime = runtime();
    let program = kernel_program(&runtime);
    let [uniform, input, output] =
        ["u", "i", "o"].map(|name| runtime.create_buffer(name, &[0; 64]).unwrap());
    let bindings = [
        BoundResource::buffer(0, 3, uniform),
        BoundResource::buffer(0, 5, input),
        BoundResource::buffer(0, 7, output),
    ];
    let request = DispatchRequest {
        program: &program,
        bindings: &bindings,
        groups: [1, 1, 1],
    };
    for _ in 0..2 {
        run(&mut runtime, |runtime, encoder| {
            runtime.dispatch(encoder, &request)
        })
        .unwrap();
    }
    assert_eq!(
        (runtime.cached_pipelines(), runtime.cached_modules()),
        (1, 1)
    );
    // A program built again from the same SPIR-V has the same content.
    let again = kernel_program(&runtime);
    run(&mut runtime, |runtime, encoder| {
        runtime.dispatch(
            encoder,
            &DispatchRequest {
                program: &again,
                ..request
            },
        )
    })
    .unwrap();
    assert_eq!(
        (runtime.cached_pipelines(), runtime.cached_modules()),
        (1, 1)
    );
}

#[test]
fn a_stage_of_the_wrong_kind_is_a_named_error() {
    let runtime = runtime();
    let fragment = stage(
        "@fragment fn main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }",
        ("main", StageKind::Fragment),
        vec![],
        ProgramParameters::default(),
    );
    assert!(matches!(
        runtime.compute_program(&fragment),
        Err(ProgramError::WrongStage {
            expected: StageKind::Compute,
            found: StageKind::Fragment
        })
    ));
}
