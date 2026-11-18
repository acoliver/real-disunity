//! Imported draws in the topology they name: four vertices drawn as a strip, in order and through
//! indices of either format, against the same four as a list, and the pipelines each builds.
//!
//! The four corners are `(-1, -1)`, `(1, -1)`, `(-1, 1.5)` and `(1, 1)`. As a strip they make
//! two triangles that cover the whole 4x4 target. As a list only the first three are a triangle,
//! whose edge from `(1, -1)` to `(-1, 1.5)` passes through no pixel centre, so the pixels it
//! covers are a closed form of their centres.

use std::sync::Arc;

use wgpu::PrimitiveTopology::{LineStrip, TriangleList, TriangleStrip};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, ImportedDraw, ImportedGeometry, ImportedIndices, ImportedVertexInput,
    PreparedProgramDraw, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::{DepthState, RenderState};

use crate::common::{CLEAR, RGBA8, SIZE, added, assert_pixel, no_cull, render, signature};
use crate::support::Gpu;

const WGSL: &str = "
@vertex fn vs(@location(0) position: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";

const CORNERS: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.5, 1.0, 1.0];
const WHITE: [f32; 4] = [1.0; 4];

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

/// The program and the four corners, lent as one vertex buffer.
struct Quad {
    program: GraphicsProgram,
    inputs: Vec<ImportedVertexInput>,
}

fn quad(gpu: &Gpu, runtime: &ProgramRuntime) -> Quad {
    let program = runtime
        .graphics_program(
            &stage("vs", StageKind::Vertex),
            &stage("fs", StageKind::Fragment),
        )
        .unwrap();
    Quad {
        program,
        inputs: vec![ImportedVertexInput {
            buffer: gpu.floats(wgpu::BufferUsages::VERTEX, &CORNERS),
            stride: 8,
            step: wgpu::VertexStepMode::Vertex,
            attributes: wgpu::vertex_attr_array![0 => Float32x2].to_vec(),
        }],
    }
}

/// The four corners in order, read through indices of `format`.
fn indexed(gpu: &Gpu, format: wgpu::IndexFormat) -> ImportedGeometry {
    let bytes: Vec<u8> = match format {
        wgpu::IndexFormat::Uint16 => [0u16, 1, 2, 3]
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect(),
        wgpu::IndexFormat::Uint32 => [0u32, 1, 2, 3]
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect(),
    };
    ImportedGeometry::Indexed(ImportedIndices {
        buffer: gpu.buffer(wgpu::BufferUsages::INDEX, &bytes),
        format,
        first: 0,
        count: 4,
        base_vertex: 0,
    })
}

fn in_order() -> ImportedGeometry {
    ImportedGeometry::Vertices { first: 0, count: 4 }
}

impl Quad {
    fn prepare(
        &self,
        runtime: &mut ProgramRuntime,
        (topology, geometry): (wgpu::PrimitiveTopology, &ImportedGeometry),
        (state, signature): (&RenderState, TargetSignature),
    ) -> Result<PreparedProgramDraw, ProgramError> {
        runtime.prepare_imported_draw(
            &ImportedDraw {
                program: &self.program,
                state,
                vertex_inputs: &self.inputs,
                bindings: &[],
                imports: &[],
                sets: &[],
                topology,
                geometry,
                instances: 0..1,
                owners: &[],
            },
            signature,
        )
    }

    fn draw(
        &self,
        runtime: &mut ProgramRuntime,
        topology: wgpu::PrimitiveTopology,
        geometry: &ImportedGeometry,
    ) -> PreparedProgramDraw {
        self.prepare(runtime, (topology, geometry), (&no_cull(), signature()))
            .unwrap()
    }
}

fn assert_covered(image: &[u8], covered: impl Fn(u32, u32) -> bool) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            assert_pixel(image, (x, y), if covered(x, y) { WHITE } else { CLEAR });
        }
    }
}

/// Whether the centre of pixel `(x, y)`, row 0 at the top, lies below the edge from `(1, -1)`
/// to `(-1, 1.5)`: inside the triangle of the first three corners.
fn in_first_triangle(x: u32, y: u32) -> bool {
    let size = f64::from(SIZE);
    let centre_x = -1.0 + (2.0 * f64::from(x) + 1.0) / size;
    let centre_y = 1.0 - (2.0 * f64::from(y) + 1.0) / size;
    centre_y + 1.0 < 1.25 * (1.0 - centre_x)
}

#[test]
fn four_vertices_drawn_in_order_as_a_triangle_strip_cover_the_whole_target() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let quad = quad(&gpu, &runtime);
    let draw = quad.draw(&mut runtime, TriangleStrip, &in_order());
    assert_covered(&render(&gpu, &[], &[&draw]), |_, _| true);
}

#[test]
fn a_strip_read_through_indices_of_either_format_covers_the_whole_target() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let quad = quad(&gpu, &runtime);
    for format in [wgpu::IndexFormat::Uint16, wgpu::IndexFormat::Uint32] {
        // wgpu refuses an indexed strip whose pipeline does not name the indices' format.
        let draw = quad.draw(&mut runtime, TriangleStrip, &indexed(&gpu, format));
        assert_covered(&render(&gpu, &[], &[&draw]), |_, _| true);
    }
}

#[test]
fn the_same_four_vertices_as_a_triangle_list_cover_only_their_first_triangle() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let quad = quad(&gpu, &runtime);
    let covered = (0..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
        .filter(|&(x, y)| in_first_triangle(x, y))
        .count();
    assert_eq!(covered, 10, "the control covers part of the target");
    for geometry in [in_order(), indexed(&gpu, wgpu::IndexFormat::Uint16)] {
        let draw = quad.draw(&mut runtime, TriangleList, &geometry);
        assert_covered(&render(&gpu, &[], &[&draw]), in_first_triangle);
    }
}

#[test]
fn pipelines_differ_by_topology_and_by_a_strips_index_format_but_not_a_lists() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let quad = quad(&gpu, &runtime);
    let sixteen = indexed(&gpu, wgpu::IndexFormat::Uint16);
    let thirty_two = indexed(&gpu, wgpu::IndexFormat::Uint32);
    let mut pipelines = |topology, geometry: &ImportedGeometry| {
        let before = runtime.stats();
        quad.draw(&mut runtime, topology, geometry);
        added(before, runtime.stats())[1]
    };
    assert_eq!(pipelines(TriangleList, &in_order()), 1);
    assert_eq!(
        pipelines(TriangleList, &sixteen),
        0,
        "a list ignores indices"
    );
    assert_eq!(pipelines(TriangleList, &thirty_two), 0);
    assert_eq!(
        pipelines(TriangleStrip, &in_order()),
        1,
        "a strip is another"
    );
    assert_eq!(pipelines(TriangleStrip, &sixteen), 1);
    assert_eq!(pipelines(TriangleStrip, &thirty_two), 1);
    assert_eq!(pipelines(TriangleStrip, &sixteen), 0, "each is cached");
    assert_eq!(pipelines(TriangleStrip, &in_order()), 0);
}

#[test]
fn a_depth_bias_on_a_line_strip_is_refused_by_name_before_a_pipeline_is_built() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let quad = quad(&gpu, &runtime);
    let with_depth =
        TargetSignature::new(&[RGBA8], Some(wgpu::TextureFormat::Depth32Float), 1).unwrap();
    let mut state = no_cull();
    state.depth = Some(DepthState {
        test: wgpu::CompareFunction::Always,
        write: true,
        bias: wgpu::DepthBiasState {
            constant: 1,
            slope_scale: 0.0,
            clamp: 0.0,
        },
    });
    let before = runtime.stats();
    let error = quad
        .prepare(&mut runtime, (LineStrip, &in_order()), (&state, with_depth))
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::DepthBiasWithoutTriangles {
            topology: LineStrip
        }
    );
    assert_eq!(added(before, runtime.stats())[1], 0);
    assert!(
        quad.prepare(
            &mut runtime,
            (TriangleStrip, &in_order()),
            (&state, with_depth)
        )
        .is_ok(),
        "a strip of triangles takes the bias"
    );
}
