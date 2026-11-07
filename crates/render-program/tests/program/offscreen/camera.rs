//! A camera render: renderers drawn into a render texture with the player's own programs.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{CameraId, MaterialId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, Renderer,
};
use worldrunner_render_program::unity::offscreen::{
    CameraCauses, CameraDraws, MissingChannelDraws, MissingTextureDraws, OffscreenError,
    UnmatchedPasses,
};
use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader};

use super::rig::{BLUE, CLEAR, QUAD, RED, Rig, layer, pixel, quad, texture};
use worldrunner_render_program::unity::synthetic::shaders::camera_passes;

pub const SHADER: ShaderId = ShaderId(1);
pub const TARGET: TextureId = TextureId(10);
pub const CAMERA: CameraId = CameraId(1);
pub const LEFT: u32 = 1;
const RIGHT: u32 = 2;

pub fn alpha() -> PassSelector {
    PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Alpha".to_owned()],
    }
}

pub fn material(tint: [f32; 4], queue: i32) -> Material {
    Material {
        shader: Some(SHADER),
        keywords: Vec::new(),
        properties: layer(&[("_Tint", tint)]),
        render_queue: queue,
    }
}

pub fn renderer(material: u64, layer_index: u32, place: [f32; 4]) -> Renderer {
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

pub fn halves(rig: &mut Rig, mask: u32) -> OffscreenFrame {
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

pub fn render(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
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
fn a_camera_with_a_pass_no_material_shader_has_draws_nothing_and_counts_the_draws() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Missing".to_owned()],
    };
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.unmatched_draws(), 2);
    drop(operations);
    let image = render(&mut rig, &frame);
    assert!(image.chunks(4).all(|texel| texel == CLEAR));
}

#[test]
fn a_renderer_whose_shader_has_no_pass_for_the_camera_is_left_out_beside_one_that_has() {
    use worldrunner_render_program::unity::synthetic::shaders::blit_passes;
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    // The blit shader's passes carry no `LightMode` tag, so the camera has none to draw of it.
    let blit = blit_passes("_MainTex");
    rig.state.add_shader(
        ShaderId(2),
        ProgramShader::new(blit.shader, blit.chunks).unwrap(),
    );
    frame.materials.get_mut(&MaterialId(2)).unwrap().shader = Some(ShaderId(2));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.unmatched_draws(), 1);
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_eq!(
        pixel(&image, 8, (1, 3)),
        RED,
        "the renderer with a pass is drawn"
    );
    assert_eq!(
        pixel(&image, 8, (5, 3)),
        CLEAR,
        "the other leaves the clear color"
    );
}

#[test]
fn a_table_that_numbers_the_shader_inputs_from_13_feeds_the_locations_from_0() {
    use worldrunner_render_program::unity::synthetic::shaders::{
        POSITION_AND_UV_FROM_13, camera_pass_with_table,
    };
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    // The player's tables number a variant's inputs from 13 when the module's locations start at
    // 0: position is input 13 of the table and location 0 of the module.
    let numbered = camera_pass_with_table(&POSITION_AND_UV_FROM_13);
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(numbered.shader, numbered.chunks).unwrap(),
    );
    let image = render(&mut rig, &frame);
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(pixel(&image, 8, (x, y)), RED, "left ({x}, {y})");
            assert_eq!(pixel(&image, 8, (x + 4, y)), CLEAR, "right ({x}, {y})");
        }
    }
    frame.cameras.get_mut(&CAMERA).unwrap().culling_mask = 1 << RIGHT;
    let right = render(&mut rig, &frame);
    assert_eq!(pixel(&right, 8, (6, 3)), BLUE);
}

#[test]
fn a_table_that_numbers_inputs_both_ways_is_a_named_error() {
    use worldrunner_render_program::unity::synthetic::shaders::camera_pass_with_table;
    let mut rig = Rig::new();
    let frame = halves(&mut rig, 1 << LEFT);
    let mixed = camera_pass_with_table(&[[0, 0], [4, 14]]);
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(mixed.shader, mixed.chunks).unwrap(),
    );
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(
            error,
            OffscreenError::MixedInputNumbering { low: 0, high: 14 }
        ),
        "{error}"
    );
}

#[test]
fn a_frame_whose_draws_all_have_a_pass_counts_none_left_out() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    assert_eq!(rig.build(&frame).unwrap().unmatched_draws(), 0);
}

#[test]
fn a_draw_whose_mesh_lacks_a_channel_the_shader_reads_is_left_out_and_counted_by_the_channel() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    let mut bare = quad(0.5);
    bare.channels.retain(|channel| {
        channel.source != worldrunner_render_program::unity::synthetic::shaders::POSITION
    });
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::NEAR_QUAD, &bare)
        .unwrap();
    frame.renderers.get_mut(&RendererId(1)).unwrap().mesh = super::rig::NEAR_QUAD;
    let operations = rig.build(&frame).unwrap();
    // The table feeds shader input 0 from channel 0, which the left renderer's mesh lacks.
    assert_eq!(
        operations.missing_channel_draws(),
        vec![MissingChannelDraws {
            channel: 0,
            input: 0,
            draws: 1
        }]
    );
    assert_eq!(operations.camera_draws(), drawn(1, 0, 0, 1));
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), CLEAR, "left: no position channel");
    assert_eq!(pixel(&image, 8, (5, 3)), BLUE, "right: drawn");
}

/// The fixture with its left renderer's mesh lacking the channel the pass reads for its positions,
/// which no default stands in for, so that renderer is left out and the right one drawn.
fn left_without_a_channel(rig: &mut Rig) -> OffscreenFrame {
    let mut frame = halves(rig, (1 << LEFT) | (1 << RIGHT));
    let mut bare = quad(0.5);
    bare.channels.retain(|channel| {
        channel.source != worldrunner_render_program::unity::synthetic::shaders::POSITION
    });
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::NEAR_QUAD, &bare)
        .unwrap();
    frame.renderers.get_mut(&RendererId(1)).unwrap().mesh = super::rig::NEAR_QUAD;
    frame
}

#[test]
fn a_draw_left_out_for_want_of_a_channel_leaves_none_of_its_constants_in_the_upload() {
    let mut rig = Rig::new();
    let frame = left_without_a_channel(&mut rig);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.camera_draws(), drawn(1, 0, 0, 1));
    // The right renderer's draw alone: the material's 16 bytes at 0 and the draw's 16 bytes one
    // uniform alignment on, since each buffer starts on the alignment.
    let alignment = u64::from(rig.gpu.device.limits().min_uniform_buffer_offset_alignment);
    assert_eq!(operations.constants_bytes(), alignment + 16);
}

#[test]
fn each_camera_names_the_channels_and_textures_its_left_out_draws_lacked() {
    let mut rig = Rig::new();
    let frame = left_without_a_channel(&mut rig);
    let operations = rig.build(&frame).unwrap();
    let channels = BTreeMap::from([((0, 0), 1)]);
    assert_eq!(
        operations.camera_causes(),
        vec![CameraCauses {
            textures: BTreeMap::new(),
            channels
        }]
    );
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Gamma".to_owned()],
    };
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.camera_causes(),
        vec![CameraCauses {
            textures: BTreeMap::from([("_Albedo".to_owned(), 2)]),
            channels: BTreeMap::new()
        }]
    );
}

#[test]
fn the_program_two_draws_share_is_built_once_and_found_by_the_next_frame() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    assert_eq!(rig.build(&frame).unwrap().programs_built(), 1);
    assert_eq!(rig.build(&frame).unwrap().programs_built(), 0);
}

#[test]
fn a_draw_whose_texture_nothing_names_is_left_out_and_counted_by_the_texture() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Gamma".to_owned()],
    };
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.missing_texture_draws(),
        vec![MissingTextureDraws {
            texture: "_Albedo".to_owned(),
            draws: 2
        }]
    );
    assert_eq!(operations.unmatched_draws(), 0);
    assert_eq!(operations.camera_draws(), drawn(0, 0, 2, 0));
    drop(operations);
    let image = render(&mut rig, &frame);
    assert!(image.chunks(4).all(|texel| texel == CLEAR));
}

#[test]
fn a_draw_without_its_texture_is_left_out_beside_one_that_needs_none() {
    use worldrunner_render_program::unity::synthetic::shaders::{
        POSITION_AND_UV, camera_pass_with_table,
    };
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    // Shader 2 has only an `Alpha` pass, which needs no texture; shader 1 has `Gamma`, which
    // needs `_Albedo`, and the camera prefers `Gamma` where a shader has it.
    let plain = camera_pass_with_table(&POSITION_AND_UV);
    rig.state.add_shader(
        ShaderId(2),
        ProgramShader::new(plain.shader, plain.chunks).unwrap(),
    );
    frame.materials.get_mut(&MaterialId(2)).unwrap().shader = Some(ShaderId(2));
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Gamma".to_owned(), "Alpha".to_owned()],
    };
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.camera_draws(), drawn(1, 0, 1, 0));
    assert_eq!(operations.missing_texture_draws().len(), 1);
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), CLEAR, "left: Gamma, no _Albedo");
    assert_eq!(pixel(&image, 8, (5, 3)), BLUE, "right: Alpha, drawn");
}

/// What one camera render of the fixture recorded.
pub fn drawn(
    drawn: usize,
    unmatched: usize,
    missing_texture: usize,
    missing_channel: usize,
) -> Vec<CameraDraws> {
    vec![CameraDraws {
        camera: CAMERA,
        target: TARGET,
        drawn,
        unmatched,
        missing_texture,
        missing_channel,
    }]
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

#[test]
fn each_camera_render_reports_the_draws_it_recorded_and_the_ones_it_left_out() {
    use worldrunner_render_program::unity::synthetic::shaders::blit_passes;
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    assert_eq!(rig.build(&frame).unwrap().camera_draws(), drawn(2, 0, 0, 0));
    frame.cameras.get_mut(&CAMERA).unwrap().culling_mask = 1 << LEFT;
    assert_eq!(rig.build(&frame).unwrap().camera_draws(), drawn(1, 0, 0, 0));
    // The blit shader's passes carry no `LightMode` tag, so the second renderer is left out.
    frame.cameras.get_mut(&CAMERA).unwrap().culling_mask = (1 << LEFT) | (1 << RIGHT);
    let blit = blit_passes("_MainTex");
    rig.state.add_shader(
        ShaderId(2),
        ProgramShader::new(blit.shader, blit.chunks).unwrap(),
    );
    frame.materials.get_mut(&MaterialId(2)).unwrap().shader = Some(ShaderId(2));
    assert_eq!(rig.build(&frame).unwrap().camera_draws(), drawn(1, 1, 0, 0));
}

#[test]
fn draws_left_out_are_counted_by_the_tags_the_shaders_passes_have_against_the_ones_selected() {
    use worldrunner_render_program::unity::synthetic::shaders::blit_passes;
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    frame.cameras.get_mut(&CAMERA).unwrap().pass = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Missing".to_owned(), "Other".to_owned()],
    };
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.unmatched_passes(),
        vec![UnmatchedPasses {
            key: "LightMode".to_owned(),
            selected: vec!["Missing".to_owned(), "Other".to_owned()],
            tags: vec!["Alpha".to_owned(), "Gamma".to_owned()],
            passes: 2,
            draws: 2,
        }]
    );
    drop(operations);
    // The blit shader has passes and none with the key.
    let blit = blit_passes("_MainTex");
    let passes = blit.shader.parsed.sub_shaders[0].passes.len();
    rig.state.add_shader(
        ShaderId(2),
        ProgramShader::new(blit.shader, blit.chunks).unwrap(),
    );
    frame.materials.get_mut(&MaterialId(2)).unwrap().shader = Some(ShaderId(2));
    let found = rig.build(&frame).unwrap().unmatched_passes();
    assert_eq!(found.len(), 2, "one entry for each kind of shader");
    let untagged = found.iter().find(|entry| entry.tags.is_empty()).unwrap();
    assert_eq!((untagged.passes, untagged.draws), (passes, 1));
}

/// The camera fixture with the shader's `LightMode` tags removed from every pass.
fn untagged(rig: &mut Rig, mask: u32) -> OffscreenFrame {
    let mut synthetic = camera_passes();
    for pass in &mut synthetic.shader.parsed.sub_shaders[0].passes {
        pass.tags.clear();
        pass.state.tags.clear();
    }
    let frame = halves(rig, mask);
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    frame
}

fn counting_untagged_as(values: &[&str], untagged: &str) -> PassSelector {
    PassSelector::TagWithUntagged {
        key: "LightMode".to_owned(),
        values: values.iter().map(|&value| value.to_owned()).collect(),
        untagged: untagged.to_owned(),
    }
}

#[test]
fn a_pass_without_the_tag_is_left_out_when_the_camera_gives_untagged_passes_no_value() {
    let mut rig = Rig::new();
    let frame = untagged(&mut rig, (1 << LEFT) | (1 << RIGHT));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.unmatched_draws(), 2);
}

#[test]
fn a_pass_without_the_tag_counts_as_the_value_the_camera_gives_untagged_passes() {
    let mut rig = Rig::new();
    let mut frame = untagged(&mut rig, (1 << LEFT) | (1 << RIGHT));
    frame.cameras.get_mut(&CAMERA).unwrap().pass =
        counting_untagged_as(&["Other", "Alpha"], "Alpha");
    assert_eq!(rig.build(&frame).unwrap().unmatched_draws(), 0);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), RED, "the left renderer is drawn");
    assert_eq!(
        pixel(&image, 8, (6, 3)),
        BLUE,
        "the right renderer is drawn"
    );
    // A value the camera does not select leaves the untagged passes out again.
    frame.cameras.get_mut(&CAMERA).unwrap().pass = counting_untagged_as(&["Other"], "Alpha");
    assert_eq!(rig.build(&frame).unwrap().unmatched_draws(), 2);
}
