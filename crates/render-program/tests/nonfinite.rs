//! SPIR-V stages that use NaN and infinity constants, which compilers emit as `OpConstant` bit
//! patterns, run on the GPU as the shader wrote them.
//!
//! The modules are synthetic: WGSL compiled to SPIR-V with finite sentinel constants whose words
//! are then replaced by non-finite bit patterns. Which primitives are discarded is observed
//! through a colour target, and exact bits through a storage buffer.

use std::sync::Arc;

use wgpu::ShaderStages;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout, StorageAccess,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, DispatchRequest, DrawRequest, Geometry, ProgramRuntime, TargetImage,
};
use worldrunner_render_program::state::RenderState;

#[path = "support/sentinel.rs"]
mod sentinel;

const NAN: u32 = 0x7fc0_0123;
const NEGATIVE_NAN: u32 = 0xffc0_0abc;
const INFINITY: u32 = 0x7f80_0000;
const NEGATIVE_INFINITY: u32 = 0xff80_0000;

/// Three quads of six vertices, side by side. The middle one is written as a NaN position, as a
/// vertex stage discards an instance.
const DISCARDING_VERTEX: &str = "
fn discarded() -> vec4<f32> { return vec4<f32>(7.0); }
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let quad = i / 6u;
    if (quad == 1u) { return discarded(); }
    let c = corner[i % 6u];
    return vec4<f32>(-1.0 + (f32(quad) + c.x) * 2.0 / 3.0, c.y, 0.0, 1.0);
}";

const FILL: &str = "
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.25, 0.5, 0.75, 1.0); }";
const COLOR: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

/// Every shape a compiler emits: a composite of mixed components, a scalar stored directly, a
/// composite that initialises a local variable, and constants used by two functions.
const BITS: &str = "
@group(0) @binding(0) var<storage, read_write> out: array<vec4<f32>, 5>;
fn helper() -> vec4<f32> { return vec4<f32>(1.5, 2.5, 1.5, 2.5); }
@compute @workgroup_size(1) fn main() {
    out[0] = vec4<f32>(1.5, 2.5, 3.5, 0.25);
    var local = vec4<f32>(0.5, 4.5, 6.5, 0.125);
    out[1] = local;
    out[2] = vec4<f32>(1.5, 1.5, 2.5, 2.5);
    out[3].x = 0.5;
    out[3].y = 1.5;
    out[4] = helper();
}";

/// The module's outputs as bits, `0xcdcd_cdcd` where the shader writes nothing.
const EXPECTED_BITS: [[u32; 4]; 5] = [
    [INFINITY, NEGATIVE_INFINITY, NAN, 0x3e80_0000],
    [0x3f00_0000, NEGATIVE_NAN, 0x40d0_0000, 0x3e00_0000],
    [INFINITY, INFINITY, NEGATIVE_INFINITY, NEGATIVE_INFINITY],
    [0x3f00_0000, INFINITY, 0xcdcd_cdcd, 0xcdcd_cdcd],
    [INFINITY, NEGATIVE_INFINITY, INFINITY, NEGATIVE_INFINITY],
];

fn stage(words: Vec<u32>, entry: &str, kind: StageKind) -> StageProgram {
    stage_with(words, entry, kind, StageDeclarations::default())
}

fn stage_with(
    words: Vec<u32>,
    entry: &str,
    kind: StageKind,
    declarations: StageDeclarations,
) -> StageProgram {
    let module = StageModule {
        source: StageSource::SpirV(Arc::from(words)),
        entry_point: entry.to_owned(),
        kind,
    };
    StageProgram::new(module, declarations).unwrap_or_else(|error| panic!("{entry}: {error}"))
}

fn discarding_vertex() -> StageProgram {
    let words = sentinel::spirv(DISCARDING_VERTEX, &[(7.0, NAN)]);
    stage(words, "vs", StageKind::Vertex)
}

fn bits_kernel() -> StageProgram {
    let words = sentinel::spirv(
        BITS,
        &[
            (1.5, INFINITY),
            (2.5, NEGATIVE_INFINITY),
            (3.5, NAN),
            (4.5, NEGATIVE_NAN),
        ],
    );
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: BindingAddress { set: 0, binding: 0 },
            source: BindingSource::Unnamed,
            visibility: ShaderStages::COMPUTE,
            kind: ResourceKind::Storage {
                access: StorageAccess::ReadWrite,
                min_size: 80,
            },
        }]),
        constants: vec![],
    };
    stage_with(words, "main", StageKind::Compute, declarations)
}

fn runtime() -> ProgramRuntime {
    let instance = worldrunner_render_gpu::test_support::test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("the non-finite constant tests need a GPU adapter");
    eprintln!(
        "render-program non-finite tests adapter: {}",
        adapter.get_info().name
    );
    let (device, queue, _) =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
            .expect("the adapter gives a device");
    ProgramRuntime::new(
        device,
        SubmissionAuthority::new(queue),
        AllocationBudget(1 << 24),
    )
}

fn submit<T>(
    runtime: &mut ProgramRuntime,
    record: impl FnOnce(&mut ProgramRuntime, &mut wgpu::CommandEncoder) -> Result<T, ProgramError>,
) -> (T, wgpu::SubmissionIndex) {
    let authority = runtime.authority().clone();
    let device = runtime.device().clone();
    let mut frame = authority.begin(&device).unwrap();
    let value = record(runtime, &mut frame).unwrap();
    (value, frame.finish().submit().unwrap())
}

fn draw_quads(vertex: &StageProgram) -> TargetImage {
    let fill = StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(FILL)),
            entry_point: "fs".to_owned(),
            kind: StageKind::Fragment,
        },
        StageDeclarations::default(),
    )
    .unwrap();
    let mut runtime = runtime();
    let program = runtime.graphics_program(vertex, &fill).unwrap();
    let target = runtime
        .create_render_target("quads", [6, 2], wgpu::TextureFormat::Rgba8Unorm, None)
        .unwrap();
    submit(&mut runtime, |runtime, frame| {
        runtime.clear_target(frame, target, [0.0; 4], 1.0)
    });
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    let request = DrawRequest {
        program: &program,
        state: &state,
        target,
        vertex_inputs: &[],
        bindings: &[],
        geometry: Geometry::Vertices {
            first: 0,
            count: 18,
        },
    };
    submit(&mut runtime, |runtime, frame| runtime.draw(frame, &request));
    let (pending, index) = submit(&mut runtime, |runtime, frame| {
        runtime.read_target(frame, target)
    });
    pending.resolve(index).unwrap()
}

fn assert_pixel(image: &TargetImage, (x, y): (u32, u32), want: [f32; 4]) {
    for (channel, (&code, value)) in image.pixel(x, y).iter().zip(want).enumerate() {
        assert_code_within(
            code,
            f64::from(value * 255.0),
            0.0,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

#[test]
fn a_vertex_stage_that_writes_a_nan_position_is_admitted() {
    let vertex = discarding_vertex();
    assert_eq!(vertex.module().entry_point, "vs");
}

#[test]
fn a_nan_position_discards_its_primitives_and_the_others_draw() {
    let image = draw_quads(&discarding_vertex());
    for y in 0..2 {
        for x in [0, 1, 4, 5] {
            assert_pixel(&image, (x, y), COLOR);
        }
        for x in [2, 3] {
            assert_pixel(&image, (x, y), [0.0; 4]);
        }
    }
}

#[test]
fn infinities_and_nans_keep_their_exact_bits_in_every_shape() {
    let kernel = bits_kernel();
    let mut runtime = runtime();
    let program = runtime.compute_program(&kernel).unwrap();
    let output = runtime.create_buffer("bits", &[0xcd; 80]).unwrap();
    let request = DispatchRequest {
        program: &program,
        bindings: &[BoundResource::buffer(0, 0, output)],
        groups: [1, 1, 1],
    };
    submit(&mut runtime, |runtime, frame| {
        runtime.dispatch(frame, &request)
    });
    let (pending, index) = submit(&mut runtime, |runtime, frame| {
        runtime.read_buffer(frame, output)
    });
    let bytes = pending.resolve(index).unwrap();
    let bits: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    let expected: Vec<u32> = EXPECTED_BITS.iter().flatten().copied().collect();
    let show = |words: &[u32]| words.iter().map(|w| format!("{w:08x}")).collect::<Vec<_>>();
    assert_eq!(show(&bits), show(&expected));
}
