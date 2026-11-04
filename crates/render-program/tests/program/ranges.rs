//! Draw ranges are checked against the bound buffers before anything is recorded.

use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{Geometry, ProgramRuntime, TargetImage, VertexInput};
use worldrunner_render_program::state::{RenderState, resolve_state};

use crate::common::{Form, opaque_state, runtime};
use crate::rig::{
    MULTIPLY, QUAD, Samplers, Scene, Setup, TEXELS, TINT_VALUE, index_buffer, pass, properties,
    setup,
};

/// A quad scene on a target cleared to transparent black, and what to draw it with.
struct Rig {
    runtime: ProgramRuntime,
    setup: Setup,
    state: RenderState,
}

fn rig() -> Rig {
    let mut runtime = runtime();
    let (material, globals) = properties();
    let state = resolve_state(&opaque_state(), &[]).unwrap().state;
    let setup = setup(
        &mut runtime,
        &Scene {
            pass: &pass(Samplers::Separate, Form::Program),
            enabled: &[MULTIPLY],
            material: &material,
            globals: &globals,
            state: &state,
            clear: [0.0; 4],
            indices: &QUAD,
        },
    );
    Rig {
        runtime,
        setup,
        state,
    }
}

impl Rig {
    /// Records the draw into a frame and submits the frame whatever the draw said, so a refused
    /// draw shows in the pixels as well as in its error.
    fn draw(&mut self, inputs: &[VertexInput], geometry: Geometry) -> Result<(), ProgramError> {
        let authority = self.runtime.authority().clone();
        let device = self.runtime.device().clone();
        let mut frame = authority.begin(&device)?;
        let request = self.setup.request(&self.state, inputs, geometry);
        let result = self.runtime.draw(&mut frame, &request);
        frame.finish().submit()?;
        result
    }

    fn image(&mut self) -> TargetImage {
        crate::common::read_target(&mut self.runtime, self.setup.target)
    }

    fn indexed(&mut self, indices: &[u16]) -> Indexed {
        Indexed(index_buffer(&mut self.runtime, indices))
    }
}

#[derive(Clone, Copy)]
struct Indexed(worldrunner_render_program::runtime::BufferHandle);

impl Indexed {
    fn range(self, format: wgpu::IndexFormat, first: u32, count: u32, base: i32) -> Geometry {
        Geometry::Indexed {
            buffer: self.0,
            format,
            first,
            count,
            base_vertex: base,
        }
    }
}

fn assert_untouched(image: &TargetImage) {
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(image.pixel(x, y), [0; 4], "pixel ({x}, {y}) was drawn");
        }
    }
}

/// The multiply variant's color of the texel under pixel `(x, y)`.
fn assert_shaded(image: &TargetImage, x: u32, y: u32) {
    let texel = TEXELS[(y * 2 + x) as usize].map(f64::from);
    for (channel, &code) in image.pixel(x, y).iter().enumerate() {
        let want = texel[channel] * f64::from(TINT_VALUE[channel]);
        assert_code_within(
            code,
            want,
            0.0,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

#[test]
fn a_vertex_range_that_overflows_is_refused_by_name_and_draws_nothing() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    let error = rig
        .draw(
            &inputs,
            Geometry::Vertices {
                first: u32::MAX,
                count: 1,
            },
        )
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::DrawRangeOverflow {
            first: u32::MAX,
            count: 1
        }
    );
    assert_untouched(&rig.image());
}

#[test]
fn an_index_range_that_overflows_is_refused_by_name() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    let indices = rig.indexed(&QUAD);
    let error = rig
        .draw(
            &inputs,
            indices.range(wgpu::IndexFormat::Uint16, u32::MAX, 2, 0),
        )
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::DrawRangeOverflow {
            first: u32::MAX,
            count: 2
        }
    );
    assert_untouched(&rig.image());
}

#[test]
fn a_vertex_range_past_the_vertex_buffer_is_refused_by_name_and_draws_nothing() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    // The quad's buffer holds four 16-byte vertices, 64 bytes; vertex 4 would end at byte 80.
    let error = rig
        .draw(&inputs, Geometry::Vertices { first: 2, count: 3 })
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::VertexRangePastBuffer {
            slot: 0,
            needed: 80,
            have: 64
        }
    );
    assert_untouched(&rig.image());
}

#[test]
fn a_vertex_range_ending_exactly_at_the_vertex_buffer_end_draws() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    // Vertices 1, 2 and 3 are the lower right triangle: it holds pixel (1, 1) and not (0, 0).
    rig.draw(&inputs, Geometry::Vertices { first: 1, count: 3 })
        .unwrap();
    let image = rig.image();
    assert_shaded(&image, 1, 1);
    assert_eq!(image.pixel(0, 0), [0; 4]);
}

#[test]
fn an_instance_buffer_is_checked_for_the_one_instance_a_draw_has() {
    let mut rig = rig();
    let corner: Vec<u8> = [-1.0_f32, -1.0, 0.0, 0.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let per_instance = |rig: &mut Rig, bytes: &[u8]| VertexInput {
        buffer: rig.runtime.create_buffer("instance", bytes).unwrap(),
        step: wgpu::VertexStepMode::Instance,
        ..rig.setup.inputs[0].clone()
    };
    // One 16-byte stride serves any number of vertices, because every vertex reads instance 0.
    let enough = [per_instance(&mut rig, &corner)];
    rig.draw(&enough, Geometry::Vertices { first: 0, count: 3 })
        .unwrap();
    let short = [per_instance(&mut rig, &corner[..8])];
    let error = rig
        .draw(&short, Geometry::Vertices { first: 0, count: 3 })
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::VertexRangePastBuffer {
            slot: 0,
            needed: 16,
            have: 8
        }
    );
}

#[test]
fn an_index_range_past_the_index_buffer_is_refused_by_name_and_draws_nothing() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    let indices = rig.indexed(&QUAD);
    let error = rig
        .draw(&inputs, indices.range(wgpu::IndexFormat::Uint16, 3, 4, 0))
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::IndexRangePastBuffer {
            end: 7,
            available: 6
        }
    );
    assert_untouched(&rig.image());
}

#[test]
fn the_index_format_sets_how_many_indices_the_buffer_holds() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    let indices = rig.indexed(&QUAD);
    // The same twelve bytes are six 16-bit indices and three 32-bit ones.
    let error = rig
        .draw(&inputs, indices.range(wgpu::IndexFormat::Uint32, 0, 6, 0))
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::IndexRangePastBuffer {
            end: 6,
            available: 3
        }
    );
}

#[test]
fn an_index_range_ending_exactly_at_the_index_buffer_end_draws() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    let indices = rig.indexed(&QUAD);
    // The second triangle, 0 2 3, is the lower left half: pixel (0, 1) and not (1, 0).
    rig.draw(&inputs, indices.range(wgpu::IndexFormat::Uint16, 3, 3, 0))
        .unwrap();
    let image = rig.image();
    assert_shaded(&image, 0, 1);
    assert_eq!(image.pixel(1, 0), [0; 4]);
}

#[test]
fn a_base_vertex_past_every_vertex_the_buffer_holds_is_refused_by_name() {
    let mut rig = rig();
    let inputs = rig.setup.inputs.clone();
    let indices = rig.indexed(&QUAD);
    let error = rig
        .draw(&inputs, indices.range(wgpu::IndexFormat::Uint16, 0, 6, 4))
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::BaseVertexPastBuffer {
            slot: 0,
            base_vertex: 4,
            vertices: 4
        }
    );
    assert_untouched(&rig.image());
}
