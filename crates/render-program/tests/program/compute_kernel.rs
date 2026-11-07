//! A compute kernel decoded from a player's compute shader, as the stage program and parameters
//! a graphics variant would give.

use worldrunner_render_program::unity::compute_kernel::{
    KernelError, kernel_stage, variant_parameters,
};
use worldrunner_render_program::unity::layout::ProgramLayout;
use worldrunner_render_program::unity::module::StageKind;
use worldrunner_render_program::unity::synthetic::stage_spirv;
use worldrunner_unity_serialized::compute_shader::{
    Code, ComputeBinding, ComputeBuffer, ComputeMember, ComputeSampler, ComputeSlot, ComputeVariant,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, VariantParameters,
};

const TEXTURE: u32 = 1 << 23;
const WRITABLE: u32 = 1 << 25;
const IMAGE: u32 = 1 << 31;

fn binding(name: &str, slot: u32, sampler: Option<u32>, dimension: Option<i32>) -> ComputeBinding {
    ComputeBinding {
        name: name.to_owned(),
        alias: String::new(),
        slot: ComputeSlot(slot),
        sampler,
        dimension,
        reserved: 0,
    }
}

fn variant(code: Vec<u8>) -> ComputeVariant {
    ComputeVariant {
        constant_buffers: vec![binding("CGlobals", 1 << 16, None, None)],
        textures: vec![
            binding("_Heights", TEXTURE, Some(3), Some(2)),
            binding("_Volume", TEXTURE | 1, None, Some(3)),
            binding("_Layers", TEXTURE | 2, None, Some(5)),
        ],
        samplers: vec![ComputeSampler {
            state: 0x54,
            bind_point: 4,
        }],
        buffers: vec![binding("_Input", 6, None, None)],
        writable: vec![
            binding("_Output", WRITABLE | 7, None, None),
            binding("_Image", IMAGE | WRITABLE | TEXTURE | 8, None, Some(2)),
        ],
        code: Code(code),
        thread_group_size: [8, 8, 1],
        requirements: 50241,
        keywords: Vec::new(),
        following: Vec::new(),
    }
}

fn layouts() -> Vec<ComputeBuffer> {
    let member = |name: &str, kind, offset, array_size, rows, columns| ComputeMember {
        name: name.to_owned(),
        kind,
        offset,
        array_size,
        rows,
        columns,
    };
    vec![ComputeBuffer {
        name: "CGlobals".to_owned(),
        size: 112,
        members: vec![
            member("_Width", 1, 0, 0, 1, 1),
            member("_Tint", 0, 16, 0, 1, 4),
            member("_Taps", 0, 32, 3, 1, 4),
            member("_Matrix", 0, 80, 0, 4, 4),
        ],
    }]
}

fn slot(set: u16, binding: u16) -> DescriptorSlot {
    DescriptorSlot {
        binding,
        set,
        stages: 0,
    }
}

fn expected_member(
    name: &str,
    shape: (i32, i32, i32, bool),
    array_size: i32,
    offset: i32,
) -> BufferMember {
    let (kind, rows, columns, is_matrix) = shape;
    BufferMember {
        name: name.to_owned(),
        kind,
        rows,
        columns,
        is_matrix,
        array_size,
        offset,
    }
}

fn resource(name: &str, slot: DescriptorSlot, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        name: name.to_owned(),
        slot,
        kind,
    }
}

#[test]
fn the_bindings_of_a_variant_become_the_parameters_of_a_graphics_variant() {
    let parameters = variant_parameters(&variant(Vec::new()), &layouts()).unwrap();
    let texture = |sampler, dimension| ResourceKind::Texture { sampler, dimension };
    let expected = VariantParameters {
        constant_buffers: vec![ConstantBuffer {
            name: "CGlobals".to_owned(),
            size: 112,
            members: vec![
                expected_member("_Width", (1, 1, 1, false), 0, 0),
                expected_member("_Tint", (0, 1, 4, false), 0, 16),
                expected_member("_Taps", (0, 1, 4, false), 3, 32),
                expected_member("_Matrix", (0, 4, 4, true), 0, 80),
            ],
            structs: Vec::new(),
        }],
        bindings: vec![
            resource("CGlobals", slot(1, 0), ResourceKind::ConstantBuffer),
            // A compute dimension of 2, 3 and 5 is the 2D, 3D and 2D array code of a graphics
            // variant: twice the number, 4, 6 and 10.
            resource("_Heights", slot(0, 0), texture(Some(slot(0, 3)), 4)),
            resource("_Volume", slot(0, 1), texture(None, 6)),
            resource("_Layers", slot(0, 2), texture(None, 10)),
            resource("", slot(0, 4), ResourceKind::Sampler { state: 0x54 }),
            resource("_Input", slot(0, 6), ResourceKind::Buffer),
            resource("_Output", slot(0, 7), ResourceKind::Buffer),
            resource("_Image", slot(0, 8), ResourceKind::StorageImage),
        ],
    };
    assert_eq!(parameters, expected);
}

#[test]
fn a_texture_that_states_no_dimension_is_refused() {
    let mut variant = variant(Vec::new());
    variant.textures[0].dimension = None;
    let error = variant_parameters(&variant, &layouts()).unwrap_err();
    assert!(
        matches!(&error, KernelError::NoDimension { name } if name == "_Heights"),
        "{error}"
    );
}

const KERNEL: &str = "
struct Globals { width: u32, height: u32 }
@group(1) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(0) var heights: texture_2d<f32>;
@group(0) @binding(6) var<storage, read> input: array<f32>;
@group(0) @binding(7) var<storage, read_write> output: array<f32>;
@compute @workgroup_size(8, 8, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let h = textureLoad(heights, vec2<i32>(id.xy), 0).x;
    output[id.y * globals.width + id.x] = h + input[id.x] + f32(globals.height);
}";

fn kernel_variant() -> ComputeVariant {
    let words = stage_spirv(KERNEL, "main", naga::ShaderStage::Compute);
    let code: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    ComputeVariant {
        textures: vec![binding("_Heights", TEXTURE, None, Some(2))],
        samplers: Vec::new(),
        buffers: vec![binding("_Input", 6, None, None)],
        writable: vec![binding("_Output", WRITABLE | 7, None, None)],
        ..variant(code)
    }
}

fn globals() -> Vec<ComputeBuffer> {
    let member = |name: &str, offset| ComputeMember {
        name: name.to_owned(),
        kind: 1,
        offset,
        array_size: 0,
        rows: 1,
        columns: 1,
    };
    vec![ComputeBuffer {
        name: "CGlobals".to_owned(),
        size: 8,
        members: vec![member("_Width", 0), member("_Height", 4)],
    }]
}

#[test]
fn a_variant_builds_a_compute_stage_whose_targets_are_named_as_the_kernel_declares_them() {
    let stage = kernel_stage(&kernel_variant(), &globals()).unwrap();
    assert_eq!(stage.module().kind, StageKind::Compute);
    assert_eq!(stage.module().entry_point, "main");

    let constants = stage.constant_buffer_targets().unwrap();
    assert_eq!(
        constants
            .iter()
            .map(|target| (target.set, target.binding))
            .collect::<Vec<_>>(),
        [(1, 0)]
    );
    let textures = stage.texture_targets().unwrap();
    assert_eq!(
        textures
            .iter()
            .map(|t| (t.name.resolve(&[]).unwrap(), t.set, t.binding))
            .collect::<Vec<_>>(),
        [("_Heights", 0, 0)]
    );
    let buffers = stage.buffer_targets().unwrap();
    assert_eq!(
        buffers
            .iter()
            .map(|t| (t.name.resolve(&[]).unwrap(), t.set, t.binding))
            .collect::<Vec<_>>(),
        [("_Input", 0, 6), ("_Output", 0, 7)]
    );
    // Every resource the module uses is declared, so the layout builds.
    let layout = ProgramLayout::build(&[&stage]).unwrap();
    assert_eq!(layout.slots().len(), 4);
}

#[test]
fn code_that_is_not_a_module_is_refused() {
    let error = kernel_stage(&variant(b"#version 430".to_vec()), &layouts()).unwrap_err();
    assert!(matches!(error, KernelError::Code(_)), "{error}");
}

#[test]
fn a_module_with_two_entry_points_is_refused() {
    let wgsl = "
    @compute @workgroup_size(1) fn first() {}
    @compute @workgroup_size(1) fn second() {}";
    let module = naga::front::wgsl::parse_str(wgsl).unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let options = naga::back::spv::Options::default();
    let words = naga::back::spv::write_vec(&module, &info, &options, None).unwrap();
    let code: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let variant = ComputeVariant {
        constant_buffers: Vec::new(),
        textures: Vec::new(),
        buffers: Vec::new(),
        writable: Vec::new(),
        ..variant(code)
    };
    let error = kernel_stage(&variant, &[]).unwrap_err();
    assert!(matches!(error, KernelError::EntryPoints(2)), "{error}");
}
