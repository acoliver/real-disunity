//! A draw recorded once over borrowed buffers: it refuses what `GeometryDraw::new` and
//! `PreparedPipeline::draw` refuse, with the same error and before any command, and records the
//! commands `GeometryDraw` records.

use std::ops::Range;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{
    BorrowedGeometry, GeometryDraw, ImportedGeometry, ImportedIndices, PassCommands,
    PreparedPipeline, ProgramRuntime, VertexBufferSpec, VertexLayoutSpec,
};

use crate::batch::{CORNERS, Commands, DRAW, Shared, layout, prepare, program, tint_set};
use crate::support::Gpu;

/// What a test needs to record draws: the pipeline and the buffers its layout reads. The
/// runtime, program and set stay alive for as long as the pipeline is drawn.
struct Fixture {
    gpu: Gpu,
    pipeline: PreparedPipeline,
    layout: VertexLayoutSpec,
    shared: Shared,
    _runtime: ProgramRuntime,
}

impl Fixture {
    fn new() -> Self {
        let gpu = Gpu::new();
        let mut runtime = gpu.runtime();
        let (program, layout, shared) = (program(&runtime), layout(), Shared::new(&gpu));
        let set = tint_set(&gpu, &mut runtime, &program);
        let pipeline = prepare(&mut runtime, (&program, &set), &layout, &[]);
        Self {
            gpu,
            pipeline,
            layout,
            shared,
            _runtime: runtime,
        }
    }

    fn buffers(&self) -> [&wgpu::Buffer; 2] {
        [&self.shared.corners, &self.shared.instances]
    }

    /// The draw made as a `GeometryDraw` and recorded through the pipeline, the way a renderer
    /// that keeps its geometry does.
    fn kept(
        &self,
        (layout, buffers): (&VertexLayoutSpec, &[&wgpu::Buffer]),
        geometry: &ImportedGeometry,
        instances: Range<u32>,
    ) -> (Result<(), ProgramError>, Log) {
        let owned: Vec<wgpu::Buffer> = buffers.iter().map(|&buffer| buffer.clone()).collect();
        let mut log = Log::default();
        let result = GeometryDraw::new(layout, &owned, geometry, instances)
            .and_then(|draw| self.pipeline.draw(&mut log, &draw));
        (result, log)
    }

    /// The same draw recorded in one call over borrowed buffers.
    fn once(
        &self,
        (layout, buffers): (&VertexLayoutSpec, &[&wgpu::Buffer]),
        geometry: &ImportedGeometry,
        instances: Range<u32>,
    ) -> (Result<(), ProgramError>, Log) {
        let mut log = Log::default();
        let result = self.pipeline.draw_borrowed(
            &mut log,
            layout,
            buffers,
            BorrowedGeometry::from(geometry),
            instances,
        );
        (result, log)
    }

    fn vertices(first: u32, count: u32) -> ImportedGeometry {
        ImportedGeometry::Vertices { first, count }
    }

    fn indexed_count(&self, count: u32) -> ImportedGeometry {
        let ImportedGeometry::Indexed(indices) = self.shared.indexed() else {
            unreachable!("the shared geometry is indexed")
        };
        ImportedGeometry::Indexed(ImportedIndices { count, ..indices })
    }
}

/// Every command with its arguments, so two recordings compare command by command.
#[derive(Default, PartialEq, Debug)]
struct Log(Vec<String>);

impl PassCommands for Log {
    fn set_pipeline(&mut self, _: &wgpu::RenderPipeline) {
        self.0.push("pipeline".to_owned());
    }
    fn set_bind_group(&mut self, set: u32, _: &wgpu::BindGroup) {
        self.0.push(format!("group {set}"));
    }
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &wgpu::Buffer) {
        self.0.push(format!("vertices {slot} {buffer:?}"));
    }
    fn set_index_buffer(&mut self, buffer: &wgpu::Buffer, format: wgpu::IndexFormat) {
        self.0.push(format!("indices {buffer:?} {format:?}"));
    }
    fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
        self.0.push(format!("draw {vertices:?} {instances:?}"));
    }
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
        self.0.push(format!(
            "draw indexed {indices:?} {base_vertex} {instances:?}"
        ));
    }
}

/// Asserts that the borrowed path answers a draw as the kept path does, and returns the answer.
fn same_answer(
    fixture: &Fixture,
    (layout, buffers): (&VertexLayoutSpec, &[&wgpu::Buffer]),
    geometry: &ImportedGeometry,
    instances: Range<u32>,
) -> Result<(), ProgramError> {
    let kept = fixture.kept((layout, buffers), geometry, instances.clone());
    let once = fixture.once((layout, buffers), geometry, instances);
    assert_eq!(once, kept);
    once.0
}

#[test]
fn a_valid_indexed_draw_records_the_commands_of_a_geometry_draw() {
    let fixture = Fixture::new();
    let geometry = fixture.shared.indexed();
    let buffers = fixture.buffers();
    for instances in [0..1, 2..3, 1..3, 0..4, 2..2] {
        let kept = fixture.kept((&fixture.layout, &buffers), &geometry, instances.clone());
        let once = fixture.once((&fixture.layout, &buffers), &geometry, instances.clone());
        assert_eq!(once.0, Ok(()), "{instances:?}");
        assert_eq!(once, kept, "{instances:?}");
        assert_eq!(once.1.0.len(), 4, "two buffers, the indices and the draw");
    }

    // The tally the other pipeline tests use sees the commands of one geometry.
    let mut commands = Commands::default();
    fixture
        .pipeline
        .draw_borrowed(
            &mut commands,
            &fixture.layout,
            &buffers,
            BorrowedGeometry::from(&geometry),
            1..2,
        )
        .unwrap();
    assert_eq!(commands.0, DRAW);
}

#[test]
fn a_valid_draw_of_vertices_records_the_commands_of_a_geometry_draw() {
    let fixture = Fixture::new();
    let buffers = fixture.buffers();
    let geometry = Fixture::vertices(1, 3);
    let once = fixture.once((&fixture.layout, &buffers), &geometry, 1..3);
    assert_eq!(
        once,
        fixture.kept((&fixture.layout, &buffers), &geometry, 1..3)
    );
    assert_eq!(once.0, Ok(()));
    assert_eq!(once.1.0.len(), 3, "two buffers and the draw");
}

#[test]
fn a_count_of_buffers_other_than_the_layouts_is_refused() {
    let fixture = Fixture::new();
    let geometry = fixture.shared.indexed();
    let one = [&fixture.shared.corners];
    let answer = same_answer(&fixture, (&fixture.layout, &one), &geometry, 0..1);
    assert_eq!(
        answer,
        Err(ProgramError::VertexBufferCount {
            expected: 2,
            found: 1
        })
    );
    let three = [
        &fixture.shared.corners,
        &fixture.shared.instances,
        &fixture.shared.instances,
    ];
    let answer = same_answer(&fixture, (&fixture.layout, &three), &geometry, 0..1);
    assert_eq!(
        answer,
        Err(ProgramError::VertexBufferCount {
            expected: 2,
            found: 3
        })
    );
}

#[test]
fn an_instance_range_that_ends_before_it_starts_is_refused() {
    let fixture = Fixture::new();
    let buffers = fixture.buffers();
    let geometry = fixture.shared.indexed();
    let (start, end) = (3, 2);
    let answer = same_answer(&fixture, (&fixture.layout, &buffers), &geometry, start..end);
    assert_eq!(
        answer,
        Err(ProgramError::InstanceRange { start: 3, end: 2 })
    );
}

#[test]
fn geometry_that_runs_past_a_buffer_is_refused() {
    let fixture = Fixture::new();
    let buffers = fixture.buffers();
    // The corner buffer holds four vertices.
    let answer = same_answer(
        &fixture,
        (&fixture.layout, &buffers),
        &Fixture::vertices(0, 5),
        0..1,
    );
    assert!(matches!(
        answer,
        Err(ProgramError::VertexRangePastBuffer { slot: 0, .. })
    ));
    // The index buffer holds six indices.
    let answer = same_answer(
        &fixture,
        (&fixture.layout, &buffers),
        &fixture.indexed_count(7),
        0..1,
    );
    assert!(matches!(
        answer,
        Err(ProgramError::IndexRangePastBuffer { .. })
    ));
    // A range whose end overflows.
    let answer = same_answer(
        &fixture,
        (&fixture.layout, &buffers),
        &Fixture::vertices(u32::MAX, 2),
        0..1,
    );
    assert!(matches!(
        answer,
        Err(ProgramError::DrawRangeOverflow { .. })
    ));
}

#[test]
fn instances_that_run_past_a_buffer_are_refused() {
    let fixture = Fixture::new();
    let buffers = fixture.buffers();
    // The instance buffer holds four records.
    let answer = same_answer(
        &fixture,
        (&fixture.layout, &buffers),
        &fixture.shared.indexed(),
        3..5,
    );
    assert!(matches!(
        answer,
        Err(ProgramError::VertexRangePastBuffer { slot: 1, .. })
    ));
}

#[test]
fn a_draw_for_another_vertex_layout_is_refused_before_any_command() {
    let fixture = Fixture::new();
    // The same corners at a stride of 16: another layout, however alike the buffers look.
    let wide = VertexLayoutSpec::new(vec![
        VertexBufferSpec {
            stride: 16,
            ..fixture.layout.buffers()[0].clone()
        },
        fixture.layout.buffers()[1].clone(),
    ]);
    let padded: Vec<f32> = CORNERS
        .iter()
        .flat_map(|corner| [corner[0], corner[1], 0.0, 0.0])
        .collect();
    let corners = fixture.gpu.floats(wgpu::BufferUsages::VERTEX, &padded);
    let buffers = [&corners, &fixture.shared.instances];
    let geometry = fixture.shared.indexed();
    let kept = fixture.kept((&wide, &buffers), &geometry, 1..2);
    let once = fixture.once((&wide, &buffers), &geometry, 1..2);
    assert_eq!(once.0, Err(ProgramError::GeometryLayoutMismatch));
    assert_eq!(once, kept);
    assert!(once.1.0.is_empty(), "a refused draw records nothing");

    // A layout built separately with equal buffers is the layout the pipeline reads.
    let equal = layout();
    let buffers = fixture.buffers();
    let once = fixture.once((&equal, &buffers), &geometry, 1..2);
    assert_eq!(once.0, Ok(()));
}

#[test]
fn a_draw_that_fails_a_check_and_the_layout_reports_the_check_first() {
    let fixture = Fixture::new();
    let wide = VertexLayoutSpec::new(vec![VertexBufferSpec {
        stride: 16,
        ..fixture.layout.buffers()[0].clone()
    }]);
    let buffers = [&fixture.shared.corners];
    // One buffer for a layout of one is a match for the count, and the corners at a stride of 16
    // run past the buffer: the extent is refused before the layout is compared.
    let answer = same_answer(&fixture, (&wide, &buffers), &Fixture::vertices(0, 4), 0..1);
    assert!(matches!(
        answer,
        Err(ProgramError::VertexRangePastBuffer { slot: 0, .. })
    ));
}
