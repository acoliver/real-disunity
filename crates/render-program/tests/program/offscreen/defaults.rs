//! Vertex inputs a mesh has no channel for: a draw reads a constant instead of being left out.

use worldrunner_render_program::unity::offscreen::frame::{
    ChannelFormat, Mesh, MeshChannel, OffscreenFrame,
};
use worldrunner_render_program::unity::offscreen::{DefaultedChannelDraws, MissingChannelDraws};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    camera_pass_reading, tint_with_extra_input,
};

use super::camera::{LEFT, SHADER, drawn, halves, render};
use super::rig::{CLEAR, QUAD, RED, Rig, pixel, quad};

/// Position into input 13, the first texture coordinates into 14 and `channel` into 15, the way
/// the player numbers the inputs of most variants.
fn table(channel: i8) -> [[i8; 2]; 3] {
    [[0, 13], [4, 14], [channel, 15]]
}

const COLOUR: i8 = 3;
const NORMAL: i8 = 1;

/// The left half of the target drawn in red by a pass of `wgsl` whose third input is fed by
/// `channel`, over the mesh `mesh`.
fn left_half(rig: &mut Rig, (wgsl, channel): (&str, i8), mesh: &Mesh) -> OffscreenFrame {
    let frame = halves(rig, 1 << LEFT);
    let synthetic = camera_pass_reading(wgsl, &table(channel));
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state.set_mesh(&mut rig.runtime, QUAD, mesh).unwrap();
    frame
}

/// The quad with `channel` of `format` holding `value` at each of its four vertices.
fn with_channel(channel: i8, format: ChannelFormat, value: &[f32]) -> Mesh {
    let mut mesh = quad(0.5);
    mesh.channels.push(MeshChannel {
        source: channel,
        format,
        bytes: (0..4)
            .flat_map(|_| value.iter().flat_map(|float| float.to_le_bytes()))
            .collect(),
    });
    mesh
}

fn defaulted(channel: i8, format: wgpu::VertexFormat) -> Vec<DefaultedChannelDraws> {
    vec![DefaultedChannelDraws {
        channel,
        input: 15,
        format,
        draws: 1,
    }]
}

#[test]
fn a_mesh_without_colour_draws_as_the_same_mesh_with_white_colour_under_a_pass_that_multiplies_by_it()
 {
    let wgsl = tint_with_extra_input("vec4<f32>", "material.tint * in.extra");
    let mut rig = Rig::new();
    let bare = left_half(&mut rig, (&wgsl, COLOUR), &quad(0.5));
    let operations = rig.build(&bare).unwrap();
    assert_eq!(operations.camera_draws(), drawn(1, 0, 0, 0));
    assert_eq!(
        operations.defaulted_channel_draws(),
        defaulted(COLOUR, wgpu::VertexFormat::Float32x4)
    );
    assert_eq!(operations.missing_channel_draws(), vec![]);
    drop(operations);
    let without = render(&mut rig, &bare);

    let white = with_channel(COLOUR, ChannelFormat::Float32x4, &[1.0; 4]);
    rig.state.set_mesh(&mut rig.runtime, QUAD, &white).unwrap();
    let operations = rig.build(&bare).unwrap();
    assert_eq!(operations.defaulted_channel_draws(), vec![]);
    drop(operations);
    let with = render(&mut rig, &bare);

    assert_eq!(without, with);
    assert_eq!(pixel(&without, 8, (1, 3)), RED, "the tint times white");
    assert_eq!(
        pixel(&without, 8, (5, 3)),
        CLEAR,
        "the right half is not drawn"
    );
}

#[test]
fn the_pass_reads_a_colour_the_mesh_has() {
    // The default above is only the identity if the pass really takes part in the multiplication.
    let wgsl = tint_with_extra_input("vec4<f32>", "material.tint * in.extra");
    let mut rig = Rig::new();
    let frame = left_half(
        &mut rig,
        (&wgsl, COLOUR),
        &with_channel(COLOUR, ChannelFormat::Float32x4, &[0.0, 0.0, 0.0, 1.0]),
    );
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), [0, 0, 0, 255]);
}

#[test]
fn a_missing_input_that_is_not_colour_reads_zeros() {
    let wgsl = tint_with_extra_input("vec4<f32>", "material.tint + in.extra");
    let mut rig = Rig::new();
    let bare = left_half(&mut rig, (&wgsl, NORMAL), &quad(0.5));
    let operations = rig.build(&bare).unwrap();
    assert_eq!(
        operations.defaulted_channel_draws(),
        defaulted(NORMAL, wgpu::VertexFormat::Float32x4)
    );
    drop(operations);
    let image = render(&mut rig, &bare);
    assert_eq!(pixel(&image, 8, (1, 3)), RED, "the tint plus zeros");

    let zeros = with_channel(NORMAL, ChannelFormat::Float32x4, &[0.0; 4]);
    rig.state.set_mesh(&mut rig.runtime, QUAD, &zeros).unwrap();
    assert_eq!(render(&mut rig, &bare), image);

    let blue = with_channel(NORMAL, ChannelFormat::Float32x4, &[0.0, 0.0, 1.0, 0.0]);
    rig.state.set_mesh(&mut rig.runtime, QUAD, &blue).unwrap();
    let shifted = render(&mut rig, &bare);
    assert_eq!(
        pixel(&shifted, 8, (1, 3)),
        [255, 0, 255, 255],
        "the pass reads it"
    );
}

#[test]
fn a_default_has_the_components_the_input_declares() {
    let wgsl = tint_with_extra_input(
        "vec3<f32>",
        "vec4<f32>(material.tint.rgb * in.extra, material.tint.a)",
    );
    let mut rig = Rig::new();
    let frame = left_half(&mut rig, (&wgsl, COLOUR), &quad(0.5));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.defaulted_channel_draws(),
        defaulted(COLOUR, wgpu::VertexFormat::Float32x3)
    );
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), RED);
}

#[test]
fn an_integer_input_reads_zeros() {
    let wgsl = tint_with_extra_input("vec4<u32>", "material.tint + vec4<f32>(in.extra)");
    let mut rig = Rig::new();
    let frame = left_half(&mut rig, (&wgsl, NORMAL), &quad(0.5));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.defaulted_channel_draws(),
        defaulted(NORMAL, wgpu::VertexFormat::Uint32x4)
    );
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), RED);
}

#[test]
fn a_colour_input_that_is_not_a_float_has_no_identity_to_default_to() {
    let wgsl = tint_with_extra_input("vec4<u32>", "material.tint + vec4<f32>(in.extra)");
    let mut rig = Rig::new();
    let frame = left_half(&mut rig, (&wgsl, COLOUR), &quad(0.5));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.camera_draws(), drawn(0, 0, 0, 1));
    assert_eq!(operations.defaulted_channel_draws(), vec![]);
    assert_eq!(
        operations.missing_channel_draws(),
        vec![MissingChannelDraws {
            channel: COLOUR,
            input: 15,
            draws: 1
        }]
    );
}

#[test]
fn a_mesh_without_positions_is_still_left_out() {
    let wgsl = tint_with_extra_input("vec4<f32>", "material.tint * in.extra");
    let mut rig = Rig::new();
    let mut unplaced = with_channel(COLOUR, ChannelFormat::Float32x4, &[1.0; 4]);
    unplaced.channels.retain(|channel| channel.source != 0);
    let frame = left_half(&mut rig, (&wgsl, COLOUR), &unplaced);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.camera_draws(), drawn(0, 0, 0, 1));
    assert_eq!(operations.defaulted_channel_draws(), vec![]);
    assert_eq!(
        operations.missing_channel_draws(),
        vec![MissingChannelDraws {
            channel: 0,
            input: 13,
            draws: 1
        }]
    );
}

#[test]
fn defaulted_draws_are_counted_each_and_reported_apart_from_the_ones_left_out() {
    let wgsl = tint_with_extra_input("vec4<f32>", "material.tint * in.extra");
    let mut rig = Rig::new();
    let mut frame = left_half(&mut rig, (&wgsl, COLOUR), &quad(0.5));
    frame
        .cameras
        .get_mut(&super::camera::CAMERA)
        .unwrap()
        .culling_mask = (1 << LEFT) | (1 << (LEFT + 1));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.camera_draws(), drawn(2, 0, 0, 0));
    assert_eq!(
        operations.defaulted_channel_draws(),
        vec![DefaultedChannelDraws {
            channel: COLOUR,
            input: 15,
            format: wgpu::VertexFormat::Float32x4,
            draws: 2
        }]
    );
    assert_eq!(operations.missing_channel_draws(), vec![]);
}
