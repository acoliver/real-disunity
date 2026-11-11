//! One prepared pipeline recorded over many geometries: the commands it issues, the pixels it
//! draws and what a frame holds when it retains the pipeline once.

use std::ops::Range;
use std::sync::Arc;

use worldrunner_render_program::binding::{BindingSource, ResourceLayout};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GeometryDraw, GraphicsProgram, Import, ImportTable, ImportedDraw, ImportedGeometry,
    ImportedIndices, ImportedVertexInput, Keepalive, PassCommands, PipelineDraw, PreparedPipeline,
    PreparedSet, ProgramRuntime, RuntimeStats, VertexBufferSpec, VertexLayoutSpec,
};

use crate::common::{
    CLEAR, RGBA8, SIZE, assert_pixel, fragment_uniform, is_dropped, no_cull, probe, signature,
};
use crate::support::Gpu;

const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
struct Varying { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> }
@vertex fn vs(
    @location(0) corner: vec2<f32>,
    @location(1) offset: vec2<f32>,
    @location(2) color: vec4<f32>,
) -> Varying {
    return Varying(vec4<f32>(corner + offset, 0.0, 1.0), color);
}
@fragment fn fs(input: Varying) -> @location(0) vec4<f32> { return input.color * tint.color; }";

/// The colour each column's instance record carries, and the factor the prepared set applies.
const COLORS: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
];
const TINT: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

fn shaded(color: [f32; 4]) -> [f32; 4] {
    [
        color[0] * TINT[0],
        color[1] * TINT[1],
        color[2] * TINT[2],
        color[3] * TINT[3],
    ]
}

const CORNERS: [[f32; 2]; 4] = [[0.0, -1.0], [0.5, -1.0], [0.5, 1.0], [0.0, 1.0]];
const QUAD: [u32; 6] = [0, 1, 2, 0, 2, 3];
const INSTANCE_STRIDE: u64 = 24;

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

fn tint_semantic() -> worldrunner_render_program::binding::EngineSemantic {
    worldrunner_render_program::binding::EngineSemantic::named("batch.tint")
}

fn program(runtime: &ProgramRuntime) -> GraphicsProgram {
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(vec![fragment_uniform(
            0,
            BindingSource::Engine(tint_semantic()),
        )]),
        constants: Vec::new(),
    };
    runtime
        .graphics_program(
            &stage("vs", StageKind::Vertex, StageDeclarations::default()),
            &stage("fs", StageKind::Fragment, declarations),
        )
        .unwrap()
}

fn tint_set(gpu: &Gpu, runtime: &mut ProgramRuntime, program: &GraphicsProgram) -> PreparedSet {
    tint_set_held(gpu, runtime, program, Vec::new())
}

fn tint_set_held(
    gpu: &Gpu,
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    owners: Vec<Keepalive>,
) -> PreparedSet {
    let tint = gpu.floats(wgpu::BufferUsages::UNIFORM, &TINT);
    let table = ImportTable::new().with(Import::buffer(tint_semantic(), tint));
    runtime.prepare_set(program, 0, &table, owners).unwrap()
}

fn corner_attributes() -> Vec<wgpu::VertexAttribute> {
    wgpu::vertex_attr_array![0 => Float32x2].to_vec()
}

fn instance_attributes() -> Vec<wgpu::VertexAttribute> {
    wgpu::vertex_attr_array![1 => Float32x2, 2 => Float32x4].to_vec()
}

/// Corners at stride 8, then one instance record of offset and colour at stride 24.
fn layout() -> VertexLayoutSpec {
    VertexLayoutSpec::new(vec![
        VertexBufferSpec {
            stride: 8,
            step: wgpu::VertexStepMode::Vertex,
            attributes: corner_attributes(),
        },
        VertexBufferSpec {
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: instance_attributes(),
        },
    ])
}

/// What the geometries of a test share: corners, indices and the instance records, where record
/// `i` puts a quad in column `i` with `COLORS[i]`.
struct Shared {
    corners: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
}

impl Shared {
    fn new(gpu: &Gpu) -> Self {
        let corners = gpu.floats(wgpu::BufferUsages::VERTEX, CORNERS.as_flattened());
        let indices: Vec<u8> = QUAD.iter().flat_map(|index| index.to_le_bytes()).collect();
        let records: Vec<f32> = (0_u8..4)
            .flat_map(|column| {
                let offset = -1.0 + 0.5 * f32::from(column);
                let color = COLORS[usize::from(column)];
                [offset, 0.0, color[0], color[1], color[2], color[3]]
            })
            .collect();
        Self {
            corners,
            indices: gpu.buffer(wgpu::BufferUsages::INDEX, &indices),
            instances: gpu.floats(wgpu::BufferUsages::VERTEX, &records),
        }
    }

    fn indexed(&self) -> ImportedGeometry {
        ImportedGeometry::Indexed(ImportedIndices {
            buffer: self.indices.clone(),
            format: wgpu::IndexFormat::Uint32,
            first: 0,
            count: 6,
            base_vertex: 0,
        })
    }

    fn geometry(
        &self,
        layout: &VertexLayoutSpec,
        instances: Range<u32>,
    ) -> Result<GeometryDraw, ProgramError> {
        GeometryDraw::new(
            layout,
            &[self.corners.clone(), self.instances.clone()],
            &self.indexed(),
            instances,
        )
    }

    /// The same draw as `geometry`, as a draw of its own.
    fn separate(
        &self,
        runtime: &mut ProgramRuntime,
        (program, set): (&GraphicsProgram, &PreparedSet),
        instances: Range<u32>,
    ) -> worldrunner_render_program::runtime::PreparedProgramDraw {
        let inputs = [
            ImportedVertexInput {
                buffer: self.corners.clone(),
                stride: 8,
                step: wgpu::VertexStepMode::Vertex,
                attributes: corner_attributes(),
            },
            ImportedVertexInput {
                buffer: self.instances.clone(),
                stride: INSTANCE_STRIDE,
                step: wgpu::VertexStepMode::Instance,
                attributes: instance_attributes(),
            },
        ];
        runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program,
                    state: &no_cull(),
                    vertex_inputs: &inputs,
                    bindings: &[],
                    imports: &[],
                    sets: &[set],
                    geometry: &self.indexed(),
                    instances,
                    owners: &[],
                },
                signature(),
            )
            .unwrap()
    }
}

fn prepare(
    runtime: &mut ProgramRuntime,
    (program, set): (&GraphicsProgram, &PreparedSet),
    layout: &VertexLayoutSpec,
    owners: &[Keepalive],
) -> PreparedPipeline {
    runtime
        .prepare_pipeline(
            &PipelineDraw {
                program,
                state: &no_cull(),
                layout,
                bindings: &[],
                imports: &[],
                sets: &[set],
                owners,
            },
            signature(),
        )
        .unwrap()
}

/// Opens a frame over `view`, lets `retain` hold what the frame needs, records `record` into a
/// pass that clears to transparent black and submits.
fn submit_pass(
    gpu: &Gpu,
    view: &wgpu::TextureView,
    retain: impl FnOnce(&wgpu::CommandEncoder),
    record: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> wgpu::SubmissionIndex {
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    retain(&frame);
    {
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("batch"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..wgpu::RenderPassDescriptor::default()
        });
        record(&mut pass);
    }
    frame.finish().submit().unwrap()
}

fn render_with(
    gpu: &Gpu,
    retain: impl FnOnce(&wgpu::CommandEncoder),
    record: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> Vec<u8> {
    let target = gpu.texture(SIZE, RGBA8, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    submit_pass(gpu, &view, retain, record);
    gpu.read(&target)
}

/// Every pass command a recording issues, by kind, so a test counts what a pipeline records
/// without a GPU pass.
#[derive(Default)]
struct Commands(Vec<&'static str>);

impl PassCommands for Commands {
    fn set_pipeline(&mut self, _: &wgpu::RenderPipeline) {
        self.0.push("pipeline");
    }
    fn set_bind_group(&mut self, _: u32, _: &wgpu::BindGroup) {
        self.0.push("group");
    }
    fn set_vertex_buffer(&mut self, _: u32, _: &wgpu::Buffer) {
        self.0.push("vertices");
    }
    fn set_index_buffer(&mut self, _: &wgpu::Buffer, _: wgpu::IndexFormat) {
        self.0.push("indices");
    }
    fn draw(&mut self, _: Range<u32>, _: Range<u32>) {
        self.0.push("draw");
    }
    fn draw_indexed(&mut self, _: Range<u32>, _: i32, _: Range<u32>) {
        self.0.push("draw indexed");
    }
}

const DRAW: [&str; 4] = ["vertices", "vertices", "indices", "draw indexed"];

/// Columns `lit` hold their record's shaded colour and the rest are clear.
fn assert_columns(image: &[u8], lit: &[usize]) {
    for (column, color) in COLORS.into_iter().enumerate() {
        let want = if lit.contains(&column) {
            shaded(color)
        } else {
            CLEAR
        };
        let column = u32::try_from(column).unwrap();
        for row in 0..SIZE {
            assert_pixel(image, (column, row), want);
        }
    }
}

#[test]
fn one_pipeline_bind_serves_many_geometries_with_unchanged_pixels() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (program, layout, shared) = (program(&runtime), layout(), Shared::new(&gpu));
    let set = tint_set(&gpu, &mut runtime, &program);
    let pipeline = prepare(&mut runtime, (&program, &set), &layout, &[]);
    let geometries: Vec<_> = (0..4)
        .map(|column| shared.geometry(&layout, column..column + 1).unwrap())
        .collect();

    // One set_pipeline and one set_bind_group, then the four commands each geometry needs.
    let mut commands = Commands::default();
    pipeline.bind(&mut commands);
    for geometry in &geometries {
        pipeline.draw(&mut commands, geometry).unwrap();
    }
    let count = geometries.len();
    assert_eq!(commands.0.len(), 2 + 4 * count);
    assert_eq!(commands.0[..2], ["pipeline", "group"]);
    assert!(commands.0[2..].chunks(4).all(|draw| draw == DRAW));

    let image = render_with(
        &gpu,
        |frame| pipeline.retain(frame).unwrap(),
        |pass| {
            pipeline.bind(pass);
            for geometry in &geometries {
                pipeline.draw(pass, geometry).unwrap();
            }
        },
    );
    assert_columns(&image, &[0, 1, 2, 3]);

    // A draw per geometry, each with its own pipeline and group commands, draws the same texels.
    let separate: Vec<_> = (0..4)
        .map(|column| shared.separate(&mut runtime, (&program, &set), column..column + 1))
        .collect();
    let baseline = render_with(
        &gpu,
        |frame| separate.iter().for_each(|draw| draw.retain(frame).unwrap()),
        |pass| separate.iter().for_each(|draw| draw.record(pass)),
    );
    assert_eq!(image, baseline);
    let mut commands = Commands::default();
    for draw in &separate {
        draw.record(&mut commands);
    }
    assert_eq!(commands.0.len(), 6 * count);
}

#[test]
fn a_geometry_for_another_vertex_layout_is_refused() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (program, layout, shared) = (program(&runtime), layout(), Shared::new(&gpu));
    let set = tint_set(&gpu, &mut runtime, &program);
    let pipeline = prepare(&mut runtime, (&program, &set), &layout, &[]);

    // The same corners at a stride of 16: another layout, however alike the buffers look.
    let wide = VertexLayoutSpec::new(vec![
        VertexBufferSpec {
            stride: 16,
            ..layout.buffers()[0].clone()
        },
        layout.buffers()[1].clone(),
    ]);
    let padded: Vec<f32> = CORNERS
        .iter()
        .flat_map(|c| [c[0], c[1], 0.0, 0.0])
        .collect();
    let corners = gpu.floats(wgpu::BufferUsages::VERTEX, &padded);
    let foreign = GeometryDraw::new(
        &wide,
        &[corners, shared.instances.clone()],
        &shared.indexed(),
        1..2,
    )
    .unwrap();
    let mut commands = Commands::default();
    assert_eq!(
        pipeline.draw(&mut commands, &foreign).err(),
        Some(ProgramError::GeometryLayoutMismatch)
    );
    assert!(commands.0.is_empty(), "a refused geometry records nothing");

    // A layout built separately with equal buffers is the same layout.
    let equal = shared.geometry(&self::layout(), 1..2).unwrap();
    pipeline.draw(&mut commands, &equal).unwrap();
    assert_eq!(commands.0, DRAW);
}

#[test]
fn an_instance_range_selects_one_record_of_a_shared_buffer() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (program, layout, shared) = (program(&runtime), layout(), Shared::new(&gpu));
    let set = tint_set(&gpu, &mut runtime, &program);
    let pipeline = prepare(&mut runtime, (&program, &set), &layout, &[]);
    let draw = |range: Range<u32>| {
        let geometry = shared.geometry(&layout, range).unwrap();
        render_with(
            &gpu,
            |frame| pipeline.retain(frame).unwrap(),
            |pass| {
                pipeline.bind(pass);
                pipeline.draw(pass, &geometry).unwrap();
            },
        )
    };
    // One record draws one column, whichever record it is.
    assert_columns(&draw(2..3), &[2]);
    assert_columns(&draw(0..1), &[0]);
    // Two records are two instances of the geometry, and none draws nothing.
    assert_columns(&draw(1..3), &[1, 2]);
    assert_columns(&draw(2..2), &[]);

    // The records are checked when the geometry is made: the buffer holds four.
    let past = shared.geometry(&layout, 3..5);
    assert!(matches!(
        past.err(),
        Some(ProgramError::VertexRangePastBuffer { slot: 1, .. })
    ));
    let (start, end) = (3, 2);
    assert_eq!(
        shared.geometry(&layout, start..end).err(),
        Some(ProgramError::InstanceRange { start: 3, end: 2 })
    );
}

#[test]
fn a_geometry_that_does_not_fit_its_buffers_is_refused_when_it_is_made() {
    let gpu = Gpu::new();
    let (layout, shared) = (layout(), Shared::new(&gpu));
    // One buffer for a layout of two.
    assert_eq!(
        GeometryDraw::new(
            &layout,
            std::slice::from_ref(&shared.corners),
            &shared.indexed(),
            0..1
        )
        .err(),
        Some(ProgramError::VertexBufferCount {
            expected: 2,
            found: 1
        })
    );
    // Indices past the end of the index buffer, which holds six.
    let ImportedGeometry::Indexed(indices) = shared.indexed() else {
        unreachable!()
    };
    let past = ImportedGeometry::Indexed(ImportedIndices {
        count: 7,
        ..indices
    });
    assert!(matches!(
        GeometryDraw::new(
            &layout,
            &[shared.corners.clone(), shared.instances.clone()],
            &past,
            0..1,
        )
        .err(),
        Some(ProgramError::IndexRangePastBuffer { .. })
    ));
}

#[test]
fn retaining_the_pipeline_once_holds_every_geometry() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (program, layout) = (program(&runtime), layout());
    let target = gpu.texture(SIZE, RGBA8, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let (owner, dropped) = probe();
    let (set_owner, set_dropped) = probe();
    let submitted = {
        let shared = Shared::new(&gpu);
        let set = tint_set_held(&gpu, &mut runtime, &program, vec![set_owner.clone()]);
        let pipeline = prepare(
            &mut runtime,
            (&program, &set),
            &layout,
            std::slice::from_ref(&owner),
        );
        let geometries: Vec<_> = (0..4)
            .map(|column| shared.geometry(&layout, column..column + 1).unwrap())
            .collect();
        let index = submit_pass(
            &gpu,
            &view,
            |frame| pipeline.retain(frame).unwrap(),
            |pass| {
                pipeline.bind(pass);
                for geometry in &geometries {
                    pipeline.draw(pass, geometry).unwrap();
                }
            },
        );
        // The buffers, geometries, set and pipeline all end here, with the frame submitted.
        index
    };
    drop((owner, set_owner));
    assert!(
        !is_dropped(&dropped) && !is_dropped(&set_dropped),
        "the frame that retained the pipeline holds its owners and its sets' until the GPU is done"
    );
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
    assert!(
        is_dropped(&dropped) && is_dropped(&set_dropped),
        "the completed frame let the owners go"
    );
    assert_columns(&gpu.read(&target), &[0, 1, 2, 3]);

    // Without a retain the pipeline's owners go with it.
    let (owner, dropped) = probe();
    let set = tint_set(&gpu, &mut runtime, &program);
    let pipeline = prepare(
        &mut runtime,
        (&program, &set),
        &layout,
        std::slice::from_ref(&owner),
    );
    drop(owner);
    assert!(!is_dropped(&dropped));
    drop(pipeline);
    assert!(is_dropped(&dropped));
}

#[test]
fn steady_state_frames_build_nothing() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (program, layout, shared) = (program(&runtime), layout(), Shared::new(&gpu));
    let set = tint_set(&gpu, &mut runtime, &program);
    let pipeline = prepare(&mut runtime, (&program, &set), &layout, &[]);
    let geometries: Vec<_> = (0..4)
        .map(|column| shared.geometry(&layout, column..column + 1).unwrap())
        .collect();
    let built: RuntimeStats = runtime.stats();
    let pipelines = runtime.cached_pipelines();
    let modules = runtime.cached_modules();

    let mut frames = Vec::new();
    for _ in 0..3 {
        frames.push(render_with(
            &gpu,
            |frame| pipeline.retain(frame).unwrap(),
            |pass| {
                pipeline.bind(pass);
                for geometry in &geometries {
                    pipeline.draw(pass, geometry).unwrap();
                }
            },
        ));
    }
    assert_eq!(runtime.stats(), built);
    assert_eq!(runtime.cached_pipelines(), pipelines);
    assert_eq!(runtime.cached_modules(), modules);
    assert!(frames.windows(2).all(|pair| pair[0] == pair[1]));
    assert_columns(&frames[0], &[0, 1, 2, 3]);
}
