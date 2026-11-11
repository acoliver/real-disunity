//! Stencil state on prepared pipelines: the operations a state gives a pipeline, and the refusal
//! of a state that names stencil for a pass that has none.
//!
//! The pass owner sets the stencil reference. The expected colours and stencil values are closed
//! forms: a marking draw writes the value 1 into the columns it covers, a consuming draw over the
//! whole target tests it with `Equal` and replaces it with zero.

use std::sync::Arc;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GeometryDraw, GraphicsProgram, ImportedGeometry, PipelineDraw, PreparedPipeline,
    ProgramRuntime, TargetSignature, VertexLayoutSpec,
};
use worldrunner_render_program::state::{
    ColorState, DepthState, RasterState, RenderState, StencilState,
};

use crate::common::{RGBA8, SIZE};
use crate::support::Gpu;

const WGSL: &str = "
fn rect(x0: f32, x1: f32, index: u32) -> vec4<f32> {
    var xs = array<f32, 6>(x0, x1, x1, x0, x1, x0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.5, 1.0);
}
@vertex fn vs_left(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return rect(-1.0, 0.0, index);
}
@vertex fn vs_full(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return rect(-1.0, 1.0, index);
}
@fragment fn fs_red() -> @location(0) vec4<f32> { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }";

const D24S8: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
const RED: [u8; 4] = [255, 0, 0, 255];
const CLEAR: [u8; 4] = [0; 4];

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

fn program(runtime: &ProgramRuntime, vertex: &str) -> GraphicsProgram {
    runtime
        .graphics_program(
            &stage(vertex, StageKind::Vertex),
            &stage("fs_red", StageKind::Fragment),
        )
        .unwrap()
}

fn face(compare: wgpu::CompareFunction, pass_op: wgpu::StencilOperation) -> wgpu::StencilFaceState {
    wgpu::StencilFaceState {
        compare,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op,
    }
}

/// Depth ignored, stencil tested with `compare` and `pass_op` on both faces, and the colour
/// written under `mask`.
fn state(
    (compare, pass_op): (wgpu::CompareFunction, wgpu::StencilOperation),
    mask: wgpu::ColorWrites,
) -> RenderState {
    let face = face(compare, pass_op);
    RenderState {
        raster: RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        },
        depth: Some(DepthState {
            test: wgpu::CompareFunction::Always,
            write: false,
            bias: wgpu::DepthBiasState::default(),
        }),
        colors: vec![Some(ColorState {
            blend: None,
            write_mask: mask,
        })],
        stencil: Some(StencilState {
            ops: wgpu::StencilState {
                front: face,
                back: face,
                read_mask: 0xff,
                write_mask: 0xff,
            },
            reference: 1,
        }),
    }
}

fn marks() -> RenderState {
    state(
        (
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Replace,
        ),
        wgpu::ColorWrites::empty(),
    )
}

fn consumes() -> RenderState {
    state(
        (wgpu::CompareFunction::Equal, wgpu::StencilOperation::Zero),
        wgpu::ColorWrites::ALL,
    )
}

fn signature(depth: wgpu::TextureFormat) -> TargetSignature {
    TargetSignature::attachments(&[RGBA8], Some(depth), 1).unwrap()
}

fn pipeline(
    runtime: &mut ProgramRuntime,
    vertex: &str,
    state: &RenderState,
    depth: wgpu::TextureFormat,
) -> Result<PreparedPipeline, ProgramError> {
    let layout = VertexLayoutSpec::new(Vec::new());
    let program = program(runtime, vertex);
    runtime.prepare_pipeline(
        &PipelineDraw {
            program: &program,
            state,
            layout: &layout,
            bindings: &[],
            imports: &[],
            sets: &[],
            owners: &[],
        },
        signature(depth),
    )
}

fn six() -> GeometryDraw {
    GeometryDraw::new(
        &VertexLayoutSpec::new(Vec::new()),
        &[],
        &ImportedGeometry::Vertices { first: 0, count: 6 },
        0..1,
    )
    .unwrap()
}

/// Draws `passes` in order into one colour and depth/stencil pass with the stencil reference 1,
/// and returns the colour texels and the stencil plane, rows tightly packed.
fn render(gpu: &Gpu, passes: &[&PreparedPipeline]) -> (Vec<u8>, Vec<u8>) {
    let color = gpu.texture(SIZE, RGBA8, 1);
    let depth = gpu.texture(SIZE, D24S8, 1);
    let (color_view, depth_view) = (
        color.create_view(&wgpu::TextureViewDescriptor::default()),
        depth.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    let geometry = six();
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    {
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stencil"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        pass.set_stencil_reference(1);
        for pipeline in passes {
            pipeline.bind(&mut pass);
            pipeline.draw(&mut pass, &geometry).unwrap();
        }
    }
    frame.finish().submit().unwrap();
    (gpu.read(&color), read_stencil(gpu, &depth))
}

fn read_stencil(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<u8> {
    let padded = SIZE.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("stencil readback"),
        size: u64::from(padded * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    frame.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::StencilOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(SIZE),
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
        .flat_map(|row| row[..SIZE as usize].to_vec())
        .collect()
}

/// The texels of the 4x4 target: `left` in columns 0 and 1, `right` in columns 2 and 3.
fn halves(left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
    (0..SIZE)
        .flat_map(|_| (0..SIZE).flat_map(move |x| if x < SIZE / 2 { left } else { right }))
        .collect()
}

fn stencil_halves(left: u8, right: u8) -> Vec<u8> {
    (0..SIZE)
        .flat_map(|_| (0..SIZE).map(move |x| if x < SIZE / 2 { left } else { right }))
        .collect()
}

#[test]
fn a_marking_draw_writes_the_reference_where_it_covers() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let mark = pipeline(&mut runtime, "vs_left", &marks(), D24S8).unwrap();
    let (color, stencil) = render(&gpu, &[&mark]);
    assert_eq!(
        color,
        halves(CLEAR, CLEAR),
        "the marking draw writes no colour"
    );
    assert_eq!(stencil, stencil_halves(1, 0));
}

#[test]
fn a_consuming_draw_covers_exactly_the_marked_texels_and_clears_the_marks() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let mark = pipeline(&mut runtime, "vs_left", &marks(), D24S8).unwrap();
    let consume = pipeline(&mut runtime, "vs_full", &consumes(), D24S8).unwrap();
    let (color, stencil) = render(&gpu, &[&mark, &consume]);
    assert_eq!(color, halves(RED, CLEAR), "colour only where the mark was");
    assert_eq!(stencil, stencil_halves(0, 0), "the consumed marks are gone");
}

#[test]
fn a_consuming_draw_with_nothing_marked_draws_nothing() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let consume = pipeline(&mut runtime, "vs_full", &consumes(), D24S8).unwrap();
    let (color, stencil) = render(&gpu, &[&consume]);
    assert_eq!(color, halves(CLEAR, CLEAR));
    assert_eq!(stencil, stencil_halves(0, 0));
}

#[test]
fn states_that_differ_only_in_stencil_are_distinct_pipelines() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let plain = RenderState {
        stencil: None,
        ..consumes()
    };
    pipeline(&mut runtime, "vs_full", &plain, D24S8).unwrap();
    let before = runtime.stats().pipelines_built;
    pipeline(&mut runtime, "vs_full", &consumes(), D24S8).unwrap();
    assert_eq!(runtime.stats().pipelines_built, before + 1);
    pipeline(&mut runtime, "vs_full", &consumes(), D24S8).unwrap();
    assert_eq!(
        runtime.stats().pipelines_built,
        before + 1,
        "equal states share one"
    );
}

#[test]
fn a_stencil_state_for_a_pass_without_a_stencil_aspect_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let refusal = ProgramError::UnsupportedState {
        what: "stencil without a stencil attachment",
    };
    assert_eq!(
        pipeline(
            &mut runtime,
            "vs_full",
            &consumes(),
            wgpu::TextureFormat::Depth32Float
        )
        .unwrap_err(),
        refusal
    );
    assert_eq!(runtime.stats().pipelines_built, 0);
}
