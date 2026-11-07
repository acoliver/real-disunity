//! Programs over a pass-bound set, and the counts of what the runtime built to prepare them.
//!
//! Each program fills half of a 4x4 target: columns 0 and 1 or columns 2 and 3. The colour is a
//! product of uniform colours the tests upload, so a binding that is missing, from the wrong
//! set or built twice changes a pixel or a count.

use std::sync::Arc;

use wgpu::{ShaderStages, TextureFormat};
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, PropertyId, ResourceBinding, ResourceKind,
    ResourceLayout,
};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, GraphicsProgram, Import, ImportedDraw, ImportedGeometry, PreparedProgramDraw,
    ProgramRuntime, RuntimeStats, TargetSignature,
};
use worldrunner_render_program::state::RenderState;

use crate::support::{Attachment, Gpu};

const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> frame: Tint;
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
@fragment fn fs_scaled() -> @location(0) vec4<f32> { return frame.color * own.color; }";

const SIZE: u32 = 4;
const RGBA8: TextureFormat = TextureFormat::Rgba8Unorm;
const FRAME: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
const OWN: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
/// `FRAME * OWN`, channel by channel.
const PRODUCT: [f32; 4] = [0.125, 0.25, 0.375, 1.0];

fn at(set: u32) -> BindingAddress {
    BindingAddress { set, binding: 0 }
}

fn uniform(set: u32, source: BindingSource) -> ResourceBinding {
    ResourceBinding {
        at: at(set),
        source,
        visibility: ShaderStages::FRAGMENT,
        kind: ResourceKind::Uniform { min_size: 16 },
    }
}

fn frame_semantic() -> EngineSemantic {
    EngineSemantic::named("frame.tint")
}

/// What the fragment stage declares: `own` from a property at set 1, and `frame` from the
/// engine at set 0.
fn fragment_declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            uniform(0, BindingSource::Engine(frame_semantic())),
            uniform(1, BindingSource::Property(PropertyId::new("own"))),
        ]),
        constants: Vec::new(),
    }
}

fn stage(entry: &str, kind: StageKind, declarations: StageDeclarations) -> StageProgram {
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

fn program(runtime: &ProgramRuntime, vertex: &str) -> GraphicsProgram {
    runtime
        .graphics_program(
            &stage(vertex, StageKind::Vertex, StageDeclarations::default()),
            &stage("fs_scaled", StageKind::Fragment, fragment_declarations()),
        )
        .unwrap()
}

fn signature() -> TargetSignature {
    TargetSignature::new(&[RGBA8], None, 1).unwrap()
}

fn no_cull() -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    image[at..at + 4].try_into().unwrap()
}

fn assert_pixel(image: &[u8], (x, y): (u32, u32), want: [f32; 4]) {
    for (channel, (&code, value)) in pixel(image, x, y).iter().zip(want).enumerate() {
        assert_code_within(
            code,
            f64::from(value * 255.0),
            0.0,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

/// Everything a draw over `program` needs: the engine buffer for `frame`, the handle for `own`
/// and the state.
struct Inputs {
    imports: Vec<Import>,
    bindings: Vec<BoundResource>,
    state: RenderState,
}

fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    inputs: &Inputs,
) -> Result<PreparedProgramDraw, worldrunner_render_program::error::ProgramError> {
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

fn floats(values: [f32; 4]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// A tally of the counts a step added, so a test states the step's cost and not the running
/// total.
fn added(before: RuntimeStats, after: RuntimeStats) -> [u64; 5] {
    [
        after.modules_built - before.modules_built,
        after.pipelines_built - before.pipelines_built,
        after.bind_groups_built - before.bind_groups_built,
        after.sets_prepared - before.sets_prepared,
        after.draws_prepared - before.draws_prepared,
    ]
}

#[test]
fn stats_count_pipelines_modules_groups_and_prepared_draws() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    assert_eq!(runtime.stats(), RuntimeStats::default());

    let left = program(&runtime, "vs_left");
    let right = program(&runtime, "vs_right");
    let frame_buffer = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let own = runtime.create_buffer("own", &floats(OWN)).unwrap();
    let inputs = |state| Inputs {
        imports: vec![Import::buffer(frame_semantic(), frame_buffer.clone())],
        bindings: vec![BoundResource::buffer(1, 0, own)],
        state,
    };
    // Building programs compiles nothing: layouts only.
    assert_eq!(runtime.stats(), RuntimeStats::default());

    // [modules, pipelines, bind groups, sets, draws] added by each step.
    let first = prepare(&mut runtime, &left, &inputs(no_cull())).unwrap();
    assert_eq!(
        added(RuntimeStats::default(), runtime.stats()),
        [2, 1, 2, 0, 1]
    );

    let mark = runtime.stats();
    let second = prepare(&mut runtime, &left, &inputs(no_cull())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [0, 0, 2, 0, 1]);

    let mark = runtime.stats();
    let culled = prepare(&mut runtime, &left, &inputs(RenderState::opaque_depth())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [0, 1, 2, 0, 1]);

    let mark = runtime.stats();
    let other = prepare(&mut runtime, &right, &inputs(no_cull())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [1, 1, 2, 0, 1]);

    // A draw refused before anything is built prepares nothing and builds nothing.
    let mark = runtime.stats();
    let mut refused = inputs(no_cull());
    refused.imports.push(Import::buffer(
        EngineSemantic::named("nothing.takes.this"),
        frame_buffer.clone(),
    ));
    assert!(prepare(&mut runtime, &left, &refused).is_err());
    assert_eq!(added(mark, runtime.stats()), [0, 0, 0, 0, 0]);

    let total = runtime.stats();
    assert_eq!(total.modules_built, runtime.cached_modules() as u64);
    assert_eq!(total.pipelines_built, runtime.cached_pipelines() as u64);

    // The counted draws still draw: the left half takes `left`, the right half `right`.
    let target = gpu.texture(SIZE, RGBA8, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass(
        &[Attachment {
            view: &view,
            resolve: None,
            clear: wgpu::Color::TRANSPARENT,
        }],
        &[&first, &second, &culled, &other],
    );
    let image = gpu.read(&target);
    assert_pixel(&image, (0, 0), PRODUCT);
    assert_pixel(&image, (1, 3), PRODUCT);
    assert_pixel(&image, (2, 1), PRODUCT);
    assert_pixel(&image, (3, 2), PRODUCT);
}
