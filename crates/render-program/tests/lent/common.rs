//! The shader, declarations and helpers the lent suites share.
//!
//! Each vertex entry point fills half of a 4x4 target: columns 0 and 1 (`vs_left`) or columns 2
//! and 3 (`vs_right`). The fragment entry points return products of uniform colours the tests
//! upload, so a binding that is missing, from the wrong set or built twice changes a pixel or a
//! count.

use std::sync::Arc;

use wgpu::{ShaderStages, TextureFormat};
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, PropertyId, ResourceBinding, ResourceKind,
    ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, ExternalSet, GraphicsProgram, Import, ImportedDraw, ImportedGeometry,
    PreparedProgramDraw, ProgramRuntime, RuntimeStats, TargetSignature,
};
use worldrunner_render_program::state::RenderState;

use crate::support::{Attachment, Gpu};

pub const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> frame: Tint;
@group(0) @binding(1) var ambient: texture_2d<f32>;
@group(1) @binding(0) var<uniform> own: Tint;
fn rect(x0: f32, x1: f32, index: u32) -> vec4<f32> {
    var xs = array<f32, 6>(x0, x1, x1, x0, x1, x0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@vertex fn vs_left(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return rect(-1.0, 0.0, index);
}
@vertex fn vs_right(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return rect(0.0, 1.0, index);
}
@fragment fn fs_frame() -> @location(0) vec4<f32> { return frame.color; }
@fragment fn fs_scaled() -> @location(0) vec4<f32> { return frame.color * own.color; }
@fragment fn fs_ambient() -> @location(0) vec4<f32> {
    let level = textureLoad(ambient, vec2<i32>(0, 0), 0).r;
    return vec4<f32>(level, level, level, 1.0);
}";

pub const SIZE: u32 = 4;
pub const RGBA8: TextureFormat = TextureFormat::Rgba8Unorm;
pub const FRAME: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
pub const OWN: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
/// `FRAME * OWN`, channel by channel.
pub const PRODUCT: [f32; 4] = [0.125, 0.25, 0.375, 1.0];
pub const CLEAR: [f32; 4] = [0.0; 4];

pub fn frame_semantic() -> EngineSemantic {
    EngineSemantic::named("frame.tint")
}

fn address(set: u32, binding: u32) -> BindingAddress {
    BindingAddress { set, binding }
}

pub fn fragment_uniform(set: u32, source: BindingSource) -> ResourceBinding {
    ResourceBinding {
        at: address(set, 0),
        source,
        visibility: ShaderStages::FRAGMENT,
        kind: ResourceKind::Uniform { min_size: 16 },
    }
}

/// What the fragment stage declares when the program owns both sets: `frame` from the engine at
/// set 0 and `own` from a property at set 1.
pub fn plain_declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            fragment_uniform(0, BindingSource::Engine(frame_semantic())),
            own_binding(),
        ]),
        constants: Vec::new(),
    }
}

fn own_binding() -> ResourceBinding {
    fragment_uniform(1, BindingSource::Property(PropertyId::new("own")))
}

/// What the fragment stage declares when set 0 is lent: only `own`.
pub fn own_declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![own_binding()]),
        constants: Vec::new(),
    }
}

pub fn stage(entry: &str, kind: StageKind, declarations: StageDeclarations) -> StageProgram {
    StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(WGSL)),
            entry_point: entry.to_owned(),
            kind,
        },
        declarations,
    )
    .unwrap()
}

pub fn vertex(entry: &str) -> StageProgram {
    stage(entry, StageKind::Vertex, StageDeclarations::default())
}

pub fn fragment(entry: &str, declarations: StageDeclarations) -> StageProgram {
    stage(entry, StageKind::Fragment, declarations)
}

/// A bind group layout of one uniform buffer at binding 0, visible to `visibility`.
pub fn uniform_layout(gpu: &Gpu, visibility: ShaderStages) -> wgpu::BindGroupLayout {
    gpu.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lent"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
}

pub fn lent_layout(gpu: &Gpu) -> wgpu::BindGroupLayout {
    uniform_layout(gpu, ShaderStages::FRAGMENT)
}

/// Set 0 lent as `layout`, holding the uniform `frame` that the engine names `frame.tint`.
pub fn lent_set(layout: &wgpu::BindGroupLayout) -> ExternalSet {
    ExternalSet {
        set: 0,
        layout: layout.clone(),
        bindings: ResourceLayout::new(vec![fragment_uniform(
            0,
            BindingSource::Engine(frame_semantic()),
        )]),
    }
}

/// The group a pass owner binds at the lent set: `colour` at binding 0.
pub fn lent_group(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    color: [f32; 4],
) -> (wgpu::BindGroup, wgpu::Buffer) {
    let buffer = gpu.floats(wgpu::BufferUsages::UNIFORM, &color);
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lent"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (group, buffer)
}

/// A program with set 0 lent, over the fragment entry `fragment_entry` and the declarations the
/// stage makes for the sets it owns.
pub fn lent_program(
    runtime: &ProgramRuntime,
    (vertex_entry, fragment_entry): (&str, &str),
    declarations: StageDeclarations,
    lent: &[ExternalSet],
) -> Result<GraphicsProgram, ProgramError> {
    runtime.graphics_program_in(
        &vertex(vertex_entry),
        &fragment(fragment_entry, declarations),
        lent,
    )
}

/// A program that owns both its sets.
pub fn plain_program(runtime: &ProgramRuntime, vertex_entry: &str) -> GraphicsProgram {
    runtime
        .graphics_program(
            &vertex(vertex_entry),
            &fragment("fs_scaled", plain_declarations()),
        )
        .unwrap()
}

pub fn signature() -> TargetSignature {
    TargetSignature::new(&[RGBA8], None, 1).unwrap()
}

pub fn no_cull() -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state
}

/// What a draw over a program is given besides the program.
pub struct Inputs {
    pub imports: Vec<Import>,
    pub bindings: Vec<BoundResource>,
    pub state: RenderState,
}

impl Inputs {
    /// Only the bindings, in the culling-free state.
    pub fn bound(bindings: Vec<BoundResource>) -> Self {
        Self {
            imports: Vec::new(),
            bindings,
            state: no_cull(),
        }
    }
}

pub fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    inputs: &Inputs,
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program,
            state: &inputs.state,
            vertex_inputs: &[],
            bindings: &inputs.bindings,
            imports: &inputs.imports,
            geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
            instances: 0..1,
            owners: &[],
        },
        signature(),
    )
}

pub fn bytes(values: [f32; 4]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// What each step added to [`RuntimeStats`], as `[modules, pipelines, bind groups, sets,
/// draws]`, so a test states a step's cost and not the running total.
pub fn added(before: RuntimeStats, after: RuntimeStats) -> [u64; 5] {
    [
        after.modules_built - before.modules_built,
        after.pipelines_built - before.pipelines_built,
        after.bind_groups_built - before.bind_groups_built,
        after.sets_prepared - before.sets_prepared,
        after.draws_prepared - before.draws_prepared,
    ]
}

/// One pass over a fresh target, cleared to transparent black, with `groups` bound before the
/// draws record. Returns the target's texels.
pub fn render(
    gpu: &Gpu,
    groups: &[(u32, &wgpu::BindGroup)],
    draws: &[&PreparedProgramDraw],
) -> Vec<u8> {
    let target = gpu.texture(SIZE, RGBA8, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass_with(
        &[Attachment {
            view: &view,
            resolve: None,
            clear: wgpu::Color::TRANSPARENT,
        }],
        groups,
        draws,
    );
    gpu.read(&target)
}

pub fn assert_pixel(image: &[u8], (x, y): (u32, u32), want: [f32; 4]) {
    let at = ((y * SIZE + x) * 4) as usize;
    for (channel, (&code, value)) in image[at..at + 4].iter().zip(want).enumerate() {
        assert_code_within(
            code,
            f64::from(value * 255.0),
            0.0,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

/// The left half of the target is `left` and the right half `right`.
pub fn assert_halves(image: &[u8], left: [f32; 4], right: [f32; 4]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            assert_pixel(image, (x, y), if x < SIZE / 2 { left } else { right });
        }
    }
}
