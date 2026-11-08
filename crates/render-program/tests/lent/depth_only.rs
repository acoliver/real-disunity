//! Passes with a depth attachment and no colour: the vertex-only programs that write them, the
//! depth bias that moves what they write and the refusals that name a state or a pass that does
//! not fit.
//!
//! Depth is read back with the `DepthOnly` aspect. A `Depth16Unorm` texel is a code `c` of 16
//! bits for the depth `c / 65535`, and the expected values are closed forms of the depths the
//! vertex shaders write.

use std::sync::Arc;

use worldrunner_render_program::error::{ProgramError, UnsupportedTarget};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GeometryDraw, GraphicsProgram, ImportedDraw, ImportedGeometry, PipelineDraw,
    PreparedProgramDraw, ProgramRuntime, TargetSignature, VertexBufferSpec, VertexLayoutSpec,
};
use worldrunner_render_program::state::{DepthState, RasterState, RenderState};

use crate::common::{SIZE, added, signature};
use crate::support::Gpu;

/// Every entry fills columns it names of the 4x4 target with a constant depth, or with a depth
/// that rises along x.
const WGSL: &str = "
fn quad(x0: f32, x1: f32, z0: f32, z1: f32, index: u32) -> vec4<f32> {
    var xs = array<f32, 6>(x0, x1, x1, x0, x1, x0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let x = xs[index];
    return vec4<f32>(x, ys[index], mix(z0, z1, (x - x0) / (x1 - x0)), 1.0);
}
@vertex fn vs_near_left(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return quad(-1.0, 0.0, 0.25, 0.25, index);
}
@vertex fn vs_far(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return quad(-1.0, 1.0, 0.75, 0.75, index);
}
@vertex fn vs_flat(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return quad(-1.0, 1.0, 20000.0 / 65535.0, 20000.0 / 65535.0, index);
}
@vertex fn vs_slope(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return quad(-1.0, 1.0, 0.2, 0.6, index);
}
@vertex fn vs_buffer(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";

const D16: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;
const D32: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The value a depth attachment is cleared to, and so what an undrawn texel holds.
const CLEARED: f64 = 1.0;
/// One 16-bit depth step, `1 / 65535`.
const STEP16: f64 = 1.0 / 65535.0;
/// The smallest difference a `Depth16Unorm` buffer resolves, `2^-16`, which the constant term of
/// a depth bias is a multiple of.
const RESOLVABLE16: f64 = 1.0 / 65536.0;
/// Half a step for rounding to the nearest code, plus a hundredth of a step for the f32
/// arithmetic of the interpolated depth and of the bias (about `2^-24` relative, far below it).
const D16_TOLERANCE: f64 = 0.51 * STEP16;
/// Interpolating a constant in `f32` is exact up to a few units in the last place.
const D32_TOLERANCE: f64 = 1.0e-6;

fn stage(entry: &str, kind: StageKind) -> StageProgram {
    StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(WGSL)),
            entry_point: entry.to_owned(),
            kind,
        },
        StageDeclarations::default(),
    )
    .unwrap()
}

fn caster(runtime: &ProgramRuntime, entry: &str) -> GraphicsProgram {
    runtime
        .vertex_only_program(&stage(entry, StageKind::Vertex))
        .unwrap()
}

/// Back faces kept, `Less` depth written with `bias`, and no colour target.
fn depth_state(bias: wgpu::DepthBiasState) -> RenderState {
    RenderState {
        raster: RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        },
        depth: Some(DepthState {
            test: wgpu::CompareFunction::Less,
            write: true,
            bias,
        }),
        colors: Vec::new(),
        stencil: None,
    }
}

fn no_bias() -> wgpu::DepthBiasState {
    wgpu::DepthBiasState::default()
}

fn biased(constant: i32, slope_scale: f32) -> RenderState {
    depth_state(wgpu::DepthBiasState {
        constant,
        slope_scale,
        clamp: 0.0,
    })
}

fn depth_only(format: wgpu::TextureFormat) -> TargetSignature {
    TargetSignature::depth_only(format, 1).unwrap()
}

fn vertices(count: u32) -> ImportedGeometry {
    ImportedGeometry::Vertices { first: 0, count }
}

fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    state: &RenderState,
    signature: TargetSignature,
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program,
            state,
            vertex_inputs: &[],
            bindings: &[],
            imports: &[],
            sets: &[],
            geometry: &vertices(6),
            instances: 0..1,
            owners: &[],
        },
        signature,
    )
}

/// One pass over a depth attachment cleared to 1, with nothing else attached. `retain` runs
/// before the pass opens and `record` inside it.
fn render(
    gpu: &Gpu,
    target: &wgpu::Texture,
    retain: impl FnOnce(&wgpu::CommandEncoder),
    record: impl FnOnce(&mut wgpu::RenderPass<'_>),
) {
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    retain(&frame);
    {
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth only"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        record(&mut pass);
    }
    frame.finish().submit().unwrap();
}

/// The depths of one pass of `draws`, row by row.
fn drawn(gpu: &Gpu, format: wgpu::TextureFormat, draws: &[&PreparedProgramDraw]) -> Vec<f64> {
    let target = gpu.texture(SIZE, format, 1);
    render(
        gpu,
        &target,
        |frame| {
            for draw in draws {
                draw.retain(frame).unwrap();
            }
        },
        |pass| {
            for draw in draws {
                draw.record(pass);
            }
        },
    );
    read_depth(gpu, &target)
}

/// The depth plane of a square `Depth16Unorm` or `Depth32Float` texture, rows tightly packed.
fn read_depth(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<f64> {
    let size = texture.width();
    let texel = match texture.format() {
        D16 => 2,
        D32 => 4,
        other => panic!("{other:?} is not read here"),
    };
    let line = size * texel;
    let padded = line.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("depth readback"),
        size: u64::from(padded * size),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    frame.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size),
            },
        },
        texture.size(),
    );
    let index = frame.finish().submit().unwrap();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: None,
        })
        .unwrap();
    let mapped = staging.slice(..).get_mapped_range().unwrap().to_vec();
    staging.unmap();
    mapped
        .chunks_exact(padded as usize)
        .flat_map(|row| row[..line as usize].chunks_exact(texel as usize))
        .map(|bytes| match bytes {
            [low, high] => f64::from(u16::from_le_bytes([*low, *high])) * STEP16,
            [a, b, c, d] => f64::from(f32::from_le_bytes([*a, *b, *c, *d])),
            _ => unreachable!("texels are two or four bytes"),
        })
        .collect()
}

fn assert_depths(image: &[f64], want: impl Fn(u32, u32) -> f64, tolerance: f64, context: &str) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (got, want) = (image[(y * SIZE + x) as usize], want(x, y));
            assert!(
                (got - want).abs() <= tolerance,
                "{context}: texel ({x}, {y}) is {got}, want {want}, bound {tolerance}"
            );
        }
    }
}

#[test]
fn a_vertex_only_program_writes_the_depth_a_closed_form_predicts() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let state = depth_state(no_bias());
    let (near, far) = (caster(&runtime, "vs_near_left"), caster(&runtime, "vs_far"));
    assert!(near.fragment().is_none());
    assert_eq!(near.vertex().module().entry_point, "vs_near_left");
    let near = prepare(&mut runtime, &near, &state, depth_only(D32)).unwrap();
    let far = prepare(&mut runtime, &far, &state, depth_only(D32)).unwrap();

    // Alone, the near quad writes 0.25 in the two left columns and leaves the cleared depth.
    assert_depths(
        &drawn(&gpu, D32, &[&near]),
        |x, _| if x < SIZE / 2 { 0.25 } else { CLEARED },
        D32_TOLERANCE,
        "near",
    );
    // The far quad covers everything at 0.75, and passes the `Less` test only where nothing nearer
    // was written.
    assert_depths(
        &drawn(&gpu, D32, &[&near, &far]),
        |x, _| if x < SIZE / 2 { 0.25 } else { 0.75 },
        D32_TOLERANCE,
        "near then far",
    );
    assert_depths(
        &drawn(&gpu, D32, &[&far, &near]),
        |x, _| if x < SIZE / 2 { 0.25 } else { 0.75 },
        D32_TOLERANCE,
        "far then near",
    );
}

/// The program of `entry`, with a fragment stage or without one: a depth bias applies to both.
fn programs(runtime: &ProgramRuntime, entry: &str) -> [GraphicsProgram; 2] {
    [
        runtime
            .graphics_program(
                &stage(entry, StageKind::Vertex),
                &stage("fs", StageKind::Fragment),
            )
            .unwrap(),
        caster(runtime, entry),
    ]
}

#[test]
fn depth_bias_moves_the_written_depth_by_the_state_value() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let signature = depth_only(D16);
    let flat_depth = 20000.0 * STEP16;

    for (index, flat) in programs(&runtime, "vs_flat").iter().enumerate() {
        // A primitive of constant depth has no slope, so the bias is its constant term alone:
        // `constant` times the smallest difference the unorm buffer resolves.
        for constant in [0, 100, -100] {
            let draw = prepare(&mut runtime, flat, &biased(constant, 0.0), signature).unwrap();
            let want = flat_depth + f64::from(constant) * RESOLVABLE16;
            assert_depths(
                &drawn(&gpu, D16, &[&draw]),
                |_, _| want,
                D16_TOLERANCE,
                &format!("program {index}, constant {constant}"),
            );
        }
    }
    // The state is part of the pipeline: three biases, three pipelines for each of two programs.
    assert_eq!(runtime.stats().pipelines_built, 6);

    // A primitive whose depth rises 0.1 a texel along x has the slope 0.1 in every texel, and
    // the bias adds `slope_scale` times it. Its depth at the centre of column x is 0.25 + 0.1 x.
    for (index, slope) in programs(&runtime, "vs_slope").iter().enumerate() {
        for scale in [0.0, 2.0] {
            let draw = prepare(&mut runtime, slope, &biased(0, scale), signature).unwrap();
            assert_depths(
                &drawn(&gpu, D16, &[&draw]),
                |x, _| 0.25 + 0.1 * f64::from(x) + f64::from(scale) * 0.1,
                D16_TOLERANCE,
                &format!("program {index}, slope scale {scale}"),
            );
        }
    }
}

#[test]
fn the_prepared_pipeline_draws_a_vertex_only_program() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = runtime
        .vertex_only_program(&stage("vs_buffer", StageKind::Vertex))
        .unwrap();
    let layout = VertexLayoutSpec::new(vec![VertexBufferSpec {
        stride: 12,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    }]);
    let state = depth_state(no_bias());
    let before = runtime.stats();
    let pipeline = runtime
        .prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &state,
                layout: &layout,
                bindings: &[],
                imports: &[],
                sets: &[],
                owners: &[],
            },
            depth_only(D32),
        )
        .unwrap();
    // One module and one pipeline, no group, and a pipeline is not a draw.
    assert_eq!(added(before, runtime.stats()), [1, 1, 0, 0, 0]);

    // Two quads of one buffer each, the left at depth 0.4 and the right at 0.6.
    let geometry = |x0: f32, x1: f32, z: f32| {
        let corners = [
            (x0, -1.0),
            (x1, -1.0),
            (x1, 1.0),
            (x0, -1.0),
            (x1, 1.0),
            (x0, 1.0),
        ];
        let floats: Vec<f32> = corners.iter().flat_map(|&(x, y)| [x, y, z]).collect();
        let buffer = gpu.floats(wgpu::BufferUsages::VERTEX, &floats);
        GeometryDraw::new(&layout, &[buffer], &vertices(6), 0..1).unwrap()
    };
    let (left, right) = (geometry(-1.0, 0.0, 0.4), geometry(0.0, 1.0, 0.6));
    let target = gpu.texture(SIZE, D32, 1);
    render(
        &gpu,
        &target,
        |frame| pipeline.retain(frame).unwrap(),
        |pass| {
            pipeline.bind(pass);
            pipeline.draw(pass, &left).unwrap();
            pipeline.draw(pass, &right).unwrap();
        },
    );
    assert_depths(
        &read_depth(&gpu, &target),
        |x, _| if x < SIZE / 2 { 0.4 } else { 0.6 },
        D32_TOLERANCE,
        "prepared pipeline",
    );
    assert_eq!(runtime.stats().draws_prepared, 0);
}

#[test]
fn a_colour_state_on_a_depth_only_signature_is_refused() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let with_colour = RenderState::opaque_depth();
    // A program with a fragment stage may draw into a depth-only pass, and its state still may
    // not describe a colour target.
    let fragment = stage("fs", StageKind::Fragment);
    let coloured = runtime
        .graphics_program(&stage("vs_far", StageKind::Vertex), &fragment)
        .unwrap();
    assert_eq!(
        prepare(&mut runtime, &coloured, &with_colour, depth_only(D32)).unwrap_err(),
        ProgramError::ColorStatesOnDepthOnly { states: 1 }
    );
    let program = caster(&runtime, "vs_far");
    assert_eq!(
        prepare(&mut runtime, &program, &with_colour, depth_only(D32)).unwrap_err(),
        ProgramError::ColorStatesOnDepthOnly { states: 1 }
    );
    let layout = VertexLayoutSpec::new(Vec::new());
    let error = runtime
        .prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &with_colour,
                layout: &layout,
                bindings: &[],
                imports: &[],
                sets: &[],
                owners: &[],
            },
            depth_only(D32),
        )
        .unwrap_err();
    assert_eq!(error, ProgramError::ColorStatesOnDepthOnly { states: 1 });
    // The converse is not a depth-only pass: a colour pass needs a state for each attachment.
    assert_eq!(
        prepare(
            &mut runtime,
            &coloured,
            &depth_state(no_bias()),
            signature()
        )
        .unwrap_err(),
        ProgramError::ColorStates {
            states: 0,
            targets: 1
        }
    );
    // Nothing was built for the refusals.
    assert_eq!(runtime.stats().pipelines_built, 0);
}

#[test]
fn a_vertex_only_program_is_refused_by_name_for_a_pass_with_colour() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = caster(&runtime, "vs_far");
    assert_eq!(
        prepare(
            &mut runtime,
            &program,
            &RenderState::opaque_depth(),
            signature()
        )
        .unwrap_err(),
        ProgramError::NoFragmentForColors { colors: 1 }
    );
    assert_eq!(runtime.stats().pipelines_built, 0);
}

#[test]
fn a_program_with_a_fragment_stage_draws_into_a_depth_only_pass_too() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = runtime
        .graphics_program(
            &stage("vs_near_left", StageKind::Vertex),
            &stage("fs", StageKind::Fragment),
        )
        .unwrap();
    let draw = prepare(
        &mut runtime,
        &program,
        &depth_state(no_bias()),
        depth_only(D32),
    )
    .unwrap();
    assert_depths(
        &drawn(&gpu, D32, &[&draw]),
        |x, _| if x < SIZE / 2 { 0.25 } else { CLEARED },
        D32_TOLERANCE,
        "with a fragment stage",
    );
}

#[test]
fn a_depth_only_signature_names_what_it_refuses() {
    let signature = TargetSignature::depth_only(D32, 4).unwrap();
    assert_eq!(signature.depth(), Some(D32));
    assert_eq!(signature.samples(), 4);
    assert_eq!(signature.color_count(), 0);
    assert_eq!(signature.color(), None);
    assert_eq!(signature, TargetSignature::depth_only(D32, 4).unwrap());
    assert_ne!(signature, TargetSignature::depth_only(D32, 1).unwrap());
    assert_ne!(signature, TargetSignature::depth_only(D16, 4).unwrap());

    assert_eq!(
        TargetSignature::depth_only(wgpu::TextureFormat::Rgba8Unorm, 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::NotDepth {
            format: wgpu::TextureFormat::Rgba8Unorm
        })
    );
    for format in [
        wgpu::TextureFormat::Stencil8,
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureFormat::Depth32FloatStencil8,
    ] {
        for samples in [1, 2, 4, 8, 16] {
            assert_eq!(
                TargetSignature::depth_only(format, samples).unwrap_err(),
                ProgramError::UnsupportedTarget(UnsupportedTarget::Stencil { format })
            );
        }
    }
    assert_eq!(
        TargetSignature::depth_only(D32, 3).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::SampleCount { samples: 3 })
    );
}
