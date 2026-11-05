//! A compute kernel with Unity-style sparse bindings, run on the GPU and read back.

use worldrunner_render_program::constants::pack_layout;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{StageKind, StageParameters, StageProgram};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{
    BoundResource, BufferHandle, ComputeProgram, DispatchRequest, ProgramRuntime,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer as PlayerBuffer, ResourceKind, VariantParameters,
};
use worldrunner_unity_serialized::shader_program::{
    ConstantBuffer, ProgramParameters, UavParam, ValueParam,
};

use crate::common::{
    Form, binding, each_form, player_binding, run, runtime, same, slot, stage, submit,
    words_to_bytes,
};

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

fn int_member(name: &str, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind: 1,
        rows: 1,
        columns: 1,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

/// The kernel's bindings as a player variant describes them. A compute stage has no bit in the
/// stage mask, which is not consulted.
fn variant() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![PlayerBuffer {
            name: "Params".to_owned(),
            size: 16,
            members: vec![
                int_member("_Scale", 0),
                int_member("_Bias", 4),
                int_member("_Count", 8),
            ],
            structs: vec![],
        }],
        bindings: vec![
            player_binding("Params", slot(0, 3, 0), ResourceKind::ConstantBuffer),
            player_binding("source", slot(0, 5, 0), ResourceKind::Buffer),
            player_binding("result", slot(0, 7, 0), ResourceKind::Buffer),
        ],
    }
}

fn kernel(form: Form) -> StageProgram {
    let parameters: StageParameters = match form {
        Form::Program => parameters().into(),
        Form::Variant => variant().into(),
    };
    let table = vec![same(0, 3), same(0, 5), same(0, 7)];
    stage(KERNEL, ("main", StageKind::Compute), table, parameters)
}

#[test]
fn a_kernel_writes_the_storage_buffer_and_only_within_its_count() {
    each_form(a_kernel_writes_in);
}

fn a_kernel_writes_in(form: Form) {
    let kernel = kernel(form);
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
    let targets = kernel.constant_buffer_targets().unwrap();
    assert_eq!((targets[0].set, targets[0].binding), (0, 3));
    let packed = pack_layout(
        &targets[0].buffer,
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

    let got = read_words(&mut runtime, output);
    // Closed form: source[i] = 7i + 1, so result[i] = 3 (7i + 1) + 100 + i = 22i + 103 for the
    // first ten elements; invocations 10 and 11 run in the third group and must write nothing.
    let expected: Vec<u32> = (0..12)
        .map(|i| if i < 10 { 22 * i + 103 } else { SENTINEL })
        .collect();
    assert_eq!(got, expected);
}

fn read_words(runtime: &mut ProgramRuntime, buffer: BufferHandle) -> Vec<u32> {
    let (pending, index) = submit(runtime, |runtime, encoder| {
        runtime.read_buffer(encoder, buffer)
    })
    .unwrap();
    pending
        .resolve(index)
        .unwrap()
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn kernel_program(runtime: &ProgramRuntime, form: Form) -> ComputeProgram {
    runtime.compute_program(&kernel(form)).unwrap()
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
    each_form(a_binding_set_that_differs_from_the_programs_is_a_named_error_in);
}

fn a_binding_set_that_differs_from_the_programs_is_a_named_error_in(form: Form) {
    let mut runtime = runtime();
    let program = kernel_program(&runtime, form);
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
    each_form(a_resource_that_does_not_suit_its_binding_is_a_named_error_in);
}

fn a_resource_that_does_not_suit_its_binding_is_a_named_error_in(form: Form) {
    let mut runtime = runtime();
    let program = kernel_program(&runtime, form);
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
    each_form(equal_dispatches_share_one_pipeline_and_one_module_in);
}

fn equal_dispatches_share_one_pipeline_and_one_module_in(form: Form) {
    let mut runtime = runtime();
    let program = kernel_program(&runtime, form);
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
    let again = kernel_program(&runtime, form);
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

#[test]
fn the_storage_buffers_a_kernel_names_come_with_their_places_in_either_form() {
    use worldrunner_render_program::buffers::ParamName;
    each_form(|form| {
        let targets = kernel(form).buffer_targets().unwrap();
        let places: Vec<_> = targets
            .iter()
            .map(|target| (target.set, target.binding))
            .collect();
        assert_eq!(places, [(0, 5), (0, 7)]);
        let names: Vec<_> = targets.into_iter().map(|target| target.name).collect();
        let expected = match form {
            Form::Program => [ParamName::Index(5), ParamName::Index(7)],
            Form::Variant => [
                ParamName::Named("source".to_owned()),
                ParamName::Named("result".to_owned()),
            ],
        };
        assert_eq!(names, expected);
    });
}
