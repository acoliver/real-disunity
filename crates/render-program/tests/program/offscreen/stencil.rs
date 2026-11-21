//! Stencil state on camera draws: a draw's `Ref` is written by `Replace` and tested by a later
//! draw, per draw, in the stencil buffer of a render texture that has one.
//!
//! Every expectation is computed from the scene: a marking draw covers some columns and writes
//! its reference there, and a consuming draw over the whole target shows its color only where the
//! stencil it reads equals its own reference under its masks.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{CameraId, MaterialId, RendererId, ShaderId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, Renderer,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::camera_passes;
use worldrunner_render_program::unity::synthetic::{fixed, named};
use worldrunner_unity_serialized::shader_state::ShaderState;

use super::camera::{TARGET, alpha};
use super::rig::{BLUE, CLEAR, GREEN, QUAD, RED, Rig, layer, pixel, quad, texture};

const RED_TINT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE_TINT: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const GREEN_TINT: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

const MARK: ShaderId = ShaderId(1);
const TEST: ShaderId = ShaderId(2);
const SIZE: u32 = 8;

/// `ZTest Always`, `ZWrite Off`, no culling and a stencil reference read from `_StencilRef`.
fn pass_state(state: &mut ShaderState) {
    state.depth.cull = fixed(0.0);
    state.depth.test = fixed(8.0);
    state.depth.write = fixed(0.0);
    state.stencil.reference = named(0.0, "_StencilRef");
}

/// `Comp Always`, `Pass Replace` under `write_mask`.
fn replace(write_mask: f32) -> impl Fn(&mut ShaderState) {
    move |state| {
        state.stencil.both.compare = fixed(8.0);
        state.stencil.both.pass = fixed(2.0);
        state.stencil.write_mask = fixed(write_mask);
    }
}

/// `Comp Equal` over `read_mask`, keeping the buffer.
fn equal(read_mask: f32) -> impl Fn(&mut ShaderState) {
    move |state| {
        state.stencil.both.compare = fixed(3.0);
        state.stencil.read_mask = fixed(read_mask);
    }
}

fn shader(stencil: impl Fn(&mut ShaderState)) -> ProgramShader {
    let mut synthetic = camera_passes();
    for pass in &mut synthetic.shader.parsed.sub_shaders[0].passes {
        pass_state(&mut pass.state);
        stencil(&mut pass.state);
    }
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

/// What one draw of the scene is: the columns it covers (`first..last` of the 8 across), its tint
/// and the `_StencilRef` its material sets.
#[derive(Clone, Copy)]
struct Item {
    shader: ShaderId,
    columns: (u32, u32),
    tint: [f32; 4],
    reference: f32,
}

fn mark(columns: (u32, u32), tint: [f32; 4], reference: f32) -> Item {
    Item {
        shader: MARK,
        columns,
        tint,
        reference,
    }
}

fn consume(tint: [f32; 4], reference: f32) -> Item {
    Item {
        shader: TEST,
        columns: (0, SIZE),
        tint,
        reference,
    }
}

/// The scene's rig with the marking shader `marking` and the testing shader `testing`.
fn rig(marking: impl Fn(&mut ShaderState), testing: impl Fn(&mut ShaderState)) -> Rig {
    let mut rig = Rig::new();
    rig.state.add_shader(MARK, shader(marking));
    rig.state.add_shader(TEST, shader(testing));
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.5))
        .unwrap();
    rig
}

/// `columns` of the target's 8 across as a fraction of its width.
fn eighths(columns: u32) -> f32 {
    f32::from(u8::try_from(columns).unwrap()) / f32::from(u8::try_from(SIZE).unwrap())
}

/// A material and a renderer for each item with the layer beside it, drawn in the order given
/// (by render queue) by the cameras whose layer mask names that layer.
fn frame(items: &[(Item, u32)], cameras: &[Camera], depth_bits: u8) -> OffscreenFrame {
    let mut materials = BTreeMap::new();
    let mut renderers = BTreeMap::new();
    for ((id, queue), (item, layer_index)) in (1u64..).zip(2000i32..).zip(items) {
        let mut properties = layer(&[("_Tint", item.tint)]);
        properties
            .values
            .set("_StencilRef", PropertyValue::Float(item.reference));
        materials.insert(
            MaterialId(id),
            Material {
                shader: Some(item.shader),
                keywords: Vec::new(),
                properties,
                fallback: PropertyLayer::default(),
                below_globals: PropertyLayer::default(),
                render_queue: queue,
            },
        );
        let (first, last) = item.columns;
        let width = eighths(last - first);
        let centre = eighths(first + last) - 1.0;
        renderers.insert(
            RendererId(id),
            Renderer {
                mesh: QUAD,
                materials: vec![MaterialId(id)],
                layer: *layer_index,
                block: PropertyLayer::default(),
                slot_blocks: std::collections::BTreeMap::new(),
                per_draw: layer(&[("_Place", [width, 1.0, centre, 0.0])]),
            },
        );
    }
    let mut target = texture(10, [SIZE, SIZE]);
    target.depth_bits = depth_bits;
    let ids: Vec<_> = (1u64..).map(CameraId).take(cameras.len()).collect();
    OffscreenFrame {
        render_textures: vec![target],
        materials,
        renderers,
        cameras: ids.iter().copied().zip(cameras.iter().cloned()).collect(),
        records: ids
            .iter()
            .map(|&camera| Record::CameraRender {
                camera,
                target: TARGET,
            })
            .collect(),
        ..OffscreenFrame::default()
    }
}

/// A camera that clears what `clear_color` and `clear_depth` say and draws the layers of `mask`.
fn camera(mask: u32, (clear_color, clear_depth): (bool, bool)) -> Camera {
    Camera {
        culling_mask: mask,
        clear_color: clear_color.then_some([1.0, 1.0, 0.0, 1.0]),
        clear_depth,
        pass: alpha(),
        values: PropertyLayer::default(),
        front_face: wgpu::FrontFace::Cw,
        renderers: super::camera::fixture_renderers(),
    }
}

/// The image of one camera that clears and draws all of `items` in order.
fn render(rig: &mut Rig, items: &[Item], depth_bits: u8) -> Vec<u8> {
    let layered: Vec<_> = items.iter().map(|item| (*item, 0)).collect();
    let frame = frame(&layered, &[camera(1, (true, true))], depth_bits);
    rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

/// Whether every texel of the columns `first..last` is `expected`.
fn columns_are(image: &[u8], (first, last): (u32, u32), expected: [u8; 4]) -> bool {
    (first..last).all(|x| (0..SIZE).all(|y| pixel(image, SIZE, (x, y)) == expected))
}

const LEFT: (u32, u32) = (0, 4);
const RIGHT: (u32, u32) = (4, 8);

#[test]
fn a_replace_draw_writes_its_reference_and_a_later_equal_draw_covers_exactly_those_texels() {
    let mut rig = rig(replace(255.0), equal(255.0));
    let items = [mark(LEFT, RED_TINT, 1.0), consume(BLUE_TINT, 1.0)];
    let image = render(&mut rig, &items, 24);
    assert!(columns_are(&image, LEFT, BLUE));
    assert!(columns_are(&image, RIGHT, CLEAR));
}

#[test]
fn an_equal_draw_with_another_reference_draws_nowhere() {
    let mut rig = rig(replace(255.0), equal(255.0));
    let items = [mark(LEFT, RED_TINT, 1.0), consume(BLUE_TINT, 2.0)];
    let image = render(&mut rig, &items, 24);
    assert!(
        columns_are(&image, LEFT, RED),
        "the marking draw's own color"
    );
    assert!(columns_are(&image, RIGHT, CLEAR));
}

#[test]
fn draws_with_different_references_in_one_pass_each_write_their_own() {
    let mut rig = rig(replace(255.0), equal(255.0));
    let items = [
        mark(LEFT, RED_TINT, 3.0),
        mark(RIGHT, RED_TINT, 4.0),
        consume(BLUE_TINT, 3.0),
        consume(GREEN_TINT, 4.0),
    ];
    let image = render(&mut rig, &items, 24);
    assert!(columns_are(&image, LEFT, BLUE), "reference 3");
    assert!(columns_are(&image, RIGHT, GREEN), "reference 4");
}

#[test]
fn the_write_mask_limits_the_bits_replace_writes() {
    let mut rig = rig(replace(15.0), equal(255.0));
    // A reference of 255 under the write mask 15 stores 15.
    let stored = [mark(LEFT, RED_TINT, 255.0), consume(BLUE_TINT, 15.0)];
    let image = render(&mut rig, &stored, 24);
    assert!(columns_are(&image, LEFT, BLUE), "15 was stored");
    let unmasked = [mark(LEFT, RED_TINT, 255.0), consume(BLUE_TINT, 255.0)];
    let image = render(&mut rig, &unmasked, 24);
    assert!(columns_are(&image, LEFT, RED), "255 was not");
}

#[test]
fn the_read_mask_limits_the_bits_the_test_compares() {
    // The stored 15 and the reference 1 agree in the one bit the read mask 1 keeps.
    let mut rig = rig(replace(255.0), equal(1.0));
    let items = [mark(LEFT, RED_TINT, 15.0), consume(BLUE_TINT, 1.0)];
    let image = render(&mut rig, &items, 24);
    assert!(columns_are(&image, LEFT, BLUE));
    assert!(columns_are(&image, RIGHT, CLEAR));
    // Nothing is stored on the right, which reads 0 and disagrees in that bit.
    let items = [mark(LEFT, RED_TINT, 14.0), consume(BLUE_TINT, 1.0)];
    let image = render(&mut rig, &items, 24);
    assert!(columns_are(&image, LEFT, RED), "14 and 1 differ in bit 0");
}

#[test]
fn the_stencil_is_kept_between_passes_and_cleared_with_the_depth() {
    let mut rig = rig(replace(255.0), equal(255.0));
    let items = [(mark(LEFT, RED_TINT, 1.0), 0), (consume(BLUE_TINT, 1.0), 1)];
    let keeping = [camera(1, (true, true)), camera(2, (false, false))];
    let image = rig
        .run(&frame(&items, &keeping, 24), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    assert!(
        columns_are(&image, LEFT, BLUE),
        "the second pass reads the mark"
    );
    assert!(columns_are(&image, RIGHT, CLEAR));
    let clearing = [camera(1, (true, true)), camera(2, (false, true))];
    let image = rig
        .run(&frame(&items, &clearing, 24), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    assert!(
        columns_are(&image, LEFT, RED),
        "the second pass cleared the stencil, so the consuming draw has no match"
    );
    assert!(columns_are(&image, RIGHT, CLEAR));
}

#[test]
fn a_target_without_a_stencil_buffer_ignores_the_stencil_state_as_unity_does() {
    // No depth buffer, and 16 bits of depth, which has no stencil: the marking draw writes
    // nothing and the test, which has no buffer to read, passes everywhere.
    for depth_bits in [0, 16] {
        let mut rig = rig(replace(255.0), equal(255.0));
        let items = [mark(LEFT, RED_TINT, 1.0), consume(BLUE_TINT, 2.0)];
        let image = render(&mut rig, &items, depth_bits);
        assert!(columns_are(&image, LEFT, BLUE), "{depth_bits} bits, left");
        assert!(columns_are(&image, RIGHT, BLUE), "{depth_bits} bits, right");
    }
}
