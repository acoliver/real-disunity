//! Measured conventions for Vulkan SPIR-V: which way clip-space Y points, the depth range that
//! survives clipping, and the winding that `FrontFace` and culling use.
//!
//! The vertex stage is a Vulkan shader: `spirv` writes it without naga's Y flip, so the
//! position it outputs is the position Unity's module would output. Each test draws into an
//! 8x8 target. Solid red, green and blue are written as exactly 0.0 or 1.0 per channel, and a
//! normalised 8-bit store of those is exact, so pixels are compared byte for byte.

use wgpu::{FrontFace, VertexFormat};
use worldrunner_render_program::unity::module::StageKind;
use worldrunner_render_program::unity::runtime::{
    DrawRequest, Geometry, GraphicsProgram, ProgramRuntime, TargetHandle, VertexInput,
};
use worldrunner_render_program::unity::state::{RenderState, resolve_state};
use worldrunner_unity_serialized::shader_program::ProgramParameters;
use worldrunner_unity_serialized::shader_state::ShaderState;

use crate::common::{fixed, opaque_state, read_target, run, runtime, stage};

const FLAT: &str = "
struct VOut { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> }
@vertex fn vs_main(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>) -> VOut {
    return VOut(vec4<f32>(pos, 1.0), color);
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return in.color; }";

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const CLEAR: [u8; 4] = [0, 0, 0, 0];
const SIZE: u32 = 8;

fn program(runtime: &ProgramRuntime) -> GraphicsProgram {
    let build = |entry, kind| stage(FLAT, (entry, kind), vec![], ProgramParameters::default());
    let vertex = build("vs_main", StageKind::Vertex);
    let fragment = build("fs_main", StageKind::Fragment);
    runtime.graphics_program(&vertex, &fragment).unwrap()
}

fn state(change: impl FnOnce(&mut ShaderState)) -> RenderState {
    let mut shader_state = opaque_state();
    change(&mut shader_state);
    resolve_state(&shader_state, &[]).unwrap().state
}

fn uncull() -> RenderState {
    state(|s| s.depth.cull = fixed(0.0))
}

/// One triangle of `color`, as position and color per vertex.
fn triangle(runtime: &mut ProgramRuntime, corners: [[f32; 3]; 3], color: [f32; 4]) -> VertexInput {
    let bytes: Vec<u8> = corners
        .iter()
        .flat_map(|corner| corner.iter().chain(&color))
        .flat_map(|value| value.to_le_bytes())
        .collect();
    VertexInput {
        buffer: runtime.create_buffer("triangle", &bytes).unwrap(),
        stride: 28,
        step: wgpu::VertexStepMode::Vertex,
        attributes: vec![
            wgpu::VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 12,
                shader_location: 1,
            },
        ],
    }
}

fn draw(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    (state, front_face): (&RenderState, FrontFace),
    (target, input): (TargetHandle, &VertexInput),
) {
    run(runtime, |runtime, encoder| {
        runtime.draw(
            encoder,
            &DrawRequest {
                program,
                state,
                front_face,
                target,
                vertex_inputs: std::slice::from_ref(input),
                bindings: &[],
                geometry: Geometry::Vertices { first: 0, count: 3 },
            },
        )
    })
    .unwrap();
}

fn new_target(runtime: &mut ProgramRuntime, depth: bool) -> TargetHandle {
    let handle = runtime
        .create_render_target(
            "conventions",
            [SIZE, SIZE],
            wgpu::TextureFormat::Rgba8Unorm,
            depth.then_some(wgpu::TextureFormat::Depth32Float),
        )
        .unwrap();
    run(runtime, |runtime, encoder| {
        runtime.clear_target(encoder, handle, [0.0; 4], 1.0)
    })
    .unwrap();
    handle
}

fn bytes(color: [f32; 4]) -> [u8; 4] {
    // Only exact 0.0 and 1.0 are used, which a normalised 8-bit store represents exactly.
    color.map(|channel| if channel >= 1.0 { 255 } else { 0 })
}

#[test]
fn positive_clip_y_lands_in_the_bottom_rows_as_in_vulkan() {
    let mut runtime = runtime();
    let program = program(&runtime);
    let target = new_target(&mut runtime, false);
    // A triangle whose clip-space Y is all positive: x from -0.9 to 0.9 at y = 0.1, apex at
    // y = 0.9. Vulkan puts +Y at the bottom of the image.
    let input = triangle(
        &mut runtime,
        [[-0.9, 0.1, 0.5], [0.9, 0.1, 0.5], [0.0, 0.9, 0.5]],
        RED,
    );
    draw(
        &mut runtime,
        &program,
        (&uncull(), FrontFace::Cw),
        (target, &input),
    );
    let image = read_target(&mut runtime, target);
    // At row 6 (center y = 6.5 of 8, clip y = 0.625) the triangle spans clip x = +-0.31, which
    // is pixel columns 3 to 4 by center.
    assert_eq!(image.pixel(4, 6), bytes(RED));
    assert_eq!(image.pixel(3, 6), bytes(RED));
    for y in 0..SIZE / 2 {
        for x in 0..SIZE {
            assert_eq!(
                image.pixel(x, y),
                CLEAR,
                "the top half stays empty at ({x}, {y})"
            );
        }
    }
}

#[test]
fn the_clip_depth_range_is_zero_to_one_and_the_nearer_triangle_wins() {
    let mut runtime = runtime();
    let program = program(&runtime);
    let target = new_target(&mut runtime, true);
    let state = uncull();
    let corners = |z: f32| [[-1.0, -1.0, z], [3.0, -1.0, z], [-1.0, 3.0, z]];
    let flags = (&state, FrontFace::Cw);
    // The far triangle is drawn last; the depth test must keep the near one.
    let near = triangle(&mut runtime, corners(0.25), GREEN);
    draw(&mut runtime, &program, flags, (target, &near));
    let far = triangle(&mut runtime, corners(0.75), RED);
    draw(&mut runtime, &program, flags, (target, &far));
    let image = read_target(&mut runtime, target);
    assert_eq!(image.pixel(4, 4), bytes(GREEN));

    // Clip z outside 0..w is clipped. With OpenGL's -1..1 range, z = -0.5 would be visible.
    let behind = new_target(&mut runtime, true);
    let clipped_near = triangle(&mut runtime, corners(-0.5), RED);
    draw(&mut runtime, &program, flags, (behind, &clipped_near));
    let clipped_far = triangle(&mut runtime, corners(1.5), RED);
    draw(&mut runtime, &program, flags, (behind, &clipped_far));
    assert_eq!(read_target(&mut runtime, behind).pixel(4, 4), CLEAR);
}

#[test]
fn front_face_is_the_winding_seen_in_the_image_and_cull_back_drops_the_other() {
    let mut runtime = runtime();
    let program = program(&runtime);
    // Clockwise as displayed with row 0 at the top: top left, top right, bottom center.
    let clockwise = [[-0.8, -0.8, 0.5], [0.8, -0.8, 0.5], [0.0, 0.8, 0.5]];
    let counter = [clockwise[0], clockwise[2], clockwise[1]];
    let culled_back = state(|s| s.depth.cull = fixed(2.0));
    let culled_front = state(|s| s.depth.cull = fixed(1.0));
    let drawn = |runtime: &mut ProgramRuntime, corners, state: &RenderState, face| {
        let target = new_target(runtime, false);
        let input = triangle(runtime, corners, RED);
        draw(runtime, &program, (state, face), (target, &input));
        read_target(runtime, target).pixel(4, 3).to_vec() == bytes(RED)
    };
    // Cull Back with Cw front faces keeps the clockwise triangle and drops the other.
    assert!(drawn(&mut runtime, clockwise, &culled_back, FrontFace::Cw));
    assert!(!drawn(&mut runtime, counter, &culled_back, FrontFace::Cw));
    // Declaring counter-clockwise the front face swaps them.
    assert!(!drawn(
        &mut runtime,
        clockwise,
        &culled_back,
        FrontFace::Ccw
    ));
    assert!(drawn(&mut runtime, counter, &culled_back, FrontFace::Ccw));
    // Cull Front drops the front faces instead.
    assert!(!drawn(
        &mut runtime,
        clockwise,
        &culled_front,
        FrontFace::Cw
    ));
    assert!(drawn(&mut runtime, counter, &culled_front, FrontFace::Cw));
}
