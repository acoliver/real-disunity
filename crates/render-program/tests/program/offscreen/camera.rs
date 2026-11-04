//! A camera render: renderers drawn into a render texture with the player's own programs.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{CameraId, MaterialId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::offscreen::OffscreenError;
use worldrunner_render_program::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, Renderer,
};
use worldrunner_render_program::shader::{PassSelector, ProgramShader};

use super::rig::{BLUE, CLEAR, QUAD, RED, Rig, layer, pixel, quad, texture};
use worldrunner_render_program::synthetic::shaders::camera_passes;

const SHADER: ShaderId = ShaderId(1);
const TARGET: TextureId = TextureId(10);
const CAMERA: CameraId = CameraId(1);
const LEFT: u32 = 1;
const RIGHT: u32 = 2;

pub fn alpha() -> PassSelector {
    PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Alpha".to_owned()],
    }
}

fn material(tint: [f32; 4], queue: i32) -> Material {
    Material {
        shader: Some(SHADER),
        keywords: Vec::new(),
        properties: layer(&[("_Tint", tint)]),
        render_queue: queue,
    }
}

fn renderer(material: u64, layer_index: u32, place: [f32; 4]) -> Renderer {
    Renderer {
        mesh: QUAD,
        materials: vec![MaterialId(material)],
        layer: layer_index,
        block: PropertyLayer::default(),
        per_draw: layer(&[("_Place", place)]),
    }
}

pub fn camera(mask: u32) -> Camera {
    Camera {
        culling_mask: mask,
        clear_color: Some([1.0, 1.0, 0.0, 1.0]),
        clear_depth: true,
        pass: alpha(),
        values: PropertyLayer::default(),
        front_face: wgpu::FrontFace::Cw,
    }
}

/// An 8x8 target, a red renderer over its left half on layer 1 and a blue one over its right half
/// on layer 2.
pub fn fixture(rig: &mut Rig, mask: u32) -> OffscreenFrame {
    halves(rig, mask)
}

fn halves(rig: &mut Rig, mask: u32) -> OffscreenFrame {
    let synthetic = camera_passes();
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.5))
        .unwrap();
    OffscreenFrame {
        render_textures: vec![texture(10, [8, 8])],
        materials: BTreeMap::from([
            (MaterialId(1), material([1.0, 0.0, 0.0, 1.0], 2000)),
            (MaterialId(2), material([0.0, 0.0, 1.0, 1.0], 2000)),
        ]),
        renderers: BTreeMap::from([
            (RendererId(1), renderer(1, LEFT, [0.5, 1.0, -0.5, 0.0])),
            (RendererId(2), renderer(2, RIGHT, [0.5, 1.0, 0.5, 0.0])),
        ]),
        cameras: BTreeMap::from([(CAMERA, camera(mask))]),
        records: vec![Record::CameraRender {
            camera: CAMERA,
            target: TARGET,
        }],
        ..OffscreenFrame::default()
    }
}

fn render(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
    rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

#[test]
fn a_camera_draws_the_renderers_its_mask_selects_over_its_clear_color() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, 1 << LEFT);
    let image = render(&mut rig, &frame);
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(pixel(&image, 8, (x, y)), RED, "left ({x}, {y})");
            assert_eq!(pixel(&image, 8, (x + 4, y)), CLEAR, "right ({x}, {y})");
        }
    }
}

#[test]
fn a_mask_with_both_layers_draws_both_renderers() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (0, 0)), RED);
    assert_eq!(pixel(&image, 8, (3, 7)), RED);
    assert_eq!(pixel(&image, 8, (4, 0)), BLUE);
    assert_eq!(pixel(&image, 8, (7, 7)), BLUE);
}

#[test]
fn a_mask_with_no_layer_of_the_frame_leaves_only_the_clear_color() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, 1 << 5);
    let image = render(&mut rig, &frame);
    assert!(image.chunks(4).all(|texel| texel == CLEAR));
}

#[test]
fn a_camera_that_does_not_clear_keeps_what_the_target_holds() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    let first = render(&mut rig, &frame);
    assert_eq!(pixel(&first, 8, (6, 0)), CLEAR);
    // A second camera draws the right half without clearing, so the left half stays red.
    frame.cameras.get_mut(&CAMERA).unwrap().culling_mask = 1 << RIGHT;
    frame.cameras.get_mut(&CAMERA).unwrap().clear_color = None;
    let second = render(&mut rig, &frame);
    assert_eq!(
        pixel(&second, 8, (1, 3)),
        RED,
        "the first frame's red is kept"
    );
    assert_eq!(pixel(&second, 8, (6, 3)), BLUE);
}

#[test]
fn renderers_are_drawn_in_render_queue_order_and_a_later_one_covers_an_earlier_one() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    // Both renderers cover the whole target; the renderer listed first has the later queue.
    for (id, place) in [
        (RendererId(1), [1.0, 1.0, 0.0, 0.0]),
        (RendererId(2), [1.0, 1.0, 0.0, 0.0]),
    ] {
        frame.renderers.get_mut(&id).unwrap().per_draw = layer(&[("_Place", place)]);
    }
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .render_queue = 3000;
    frame
        .materials
        .get_mut(&MaterialId(2))
        .unwrap()
        .render_queue = 2000;
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (4, 4)), RED, "queue 3000 drawn after 2000");
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .render_queue = 1000;
    let image = render(&mut rig, &frame);
    assert_eq!(
        pixel(&image, 8, (4, 4)),
        BLUE,
        "queue 1000 drawn before 2000"
    );
}

#[test]
fn a_depth_buffer_hides_a_farther_quad_drawn_later() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    frame.render_textures[0].depth_bits = 32;
    // Both cover the target. The red one is at depth 0.8 and the blue one at 0.2.
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.8))
        .unwrap();
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::NEAR_QUAD, &quad(0.2))
        .unwrap();
    for id in [RendererId(1), RendererId(2)] {
        frame.renderers.get_mut(&id).unwrap().per_draw = layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    }
    frame.renderers.get_mut(&RendererId(2)).unwrap().mesh = super::rig::NEAR_QUAD;
    // The far quad is drawn after the near one: the depth test rejects it.
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .render_queue = 3000;
    frame
        .materials
        .get_mut(&MaterialId(2))
        .unwrap()
        .render_queue = 2000;
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (4, 4)), BLUE, "far quad drawn second");
    // Drawn first, the far quad is covered by the near one.
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .render_queue = 1000;
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (4, 4)), BLUE, "far quad drawn first");
}

#[test]
fn a_property_no_layer_supplies_is_listed_and_keeps_zero() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw = PropertyLayer::default();
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.missing(), ["_Place".to_owned()]);
    drop(operations);
    // Zero `_Place` collapses the quad, so only the clear color is left.
    let image = render(&mut rig, &frame);
    assert!(image.chunks(4).all(|texel| texel == CLEAR));
}

#[test]
fn a_camera_with_a_pass_no_material_shader_has_is_a_named_error() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Missing".to_owned()],
    };
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(
            &error,
            OffscreenError::Resolve(
                worldrunner_render_program::shader::ResolveError::NoPass { .. }
            )
        ),
        "{error}"
    );
}

#[test]
fn a_mesh_without_a_channel_the_shader_reads_is_a_named_error() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    let mut bare = quad(0.5);
    bare.channels.retain(|channel| {
        channel.source == worldrunner_render_program::synthetic::shaders::POSITION
    });
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::NEAR_QUAD, &bare)
        .unwrap();
    frame.renderers.get_mut(&RendererId(1)).unwrap().mesh = super::rig::NEAR_QUAD;
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(
            error,
            OffscreenError::MissingChannel {
                channel: 4,
                input: 1,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_texture_nothing_names_is_a_named_error() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Gamma".to_owned()],
    };
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(&error, OffscreenError::MissingTexture { name } if name == "_Albedo"),
        "{error}"
    );
}

#[test]
fn the_pass_and_the_draws_are_listed_in_call_order_with_the_constants_upload_first() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    let operations = rig.build(&frame).unwrap();
    let names: Vec<_> = operations
        .operations()
        .iter()
        .map(|operation| operation.id.stage.name)
        .collect();
    assert_eq!(names, ["offscreen constants", "offscreen camera"]);
}
