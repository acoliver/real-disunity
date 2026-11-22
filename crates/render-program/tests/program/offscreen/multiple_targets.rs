//! A pass over several colour attachments and a separate depth buffer: the fragment stage's
//! outputs each reach their own texture, a clear sets every colour, and each attachment is
//! read by later passes at the version its own last write made.

use std::sync::Arc;

use worldrunner_render_program::ids::{MaterialId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, DrawList, DrawRaster, Instances, ListDraw, OffscreenFrame,
    Record,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::SyntheticShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    POSITION_AND_UV, TINT, camera_pass_combining, camera_pass_reading, camera_pass_three_outputs,
};

use super::camera::{TARGET, alpha, fixture};
use super::rig::{BLUE, CLEAR, GREEN, RED, Rig, ZERO, pixel, texture};

const THREE: ShaderId = ShaderId(2);
const COMBINING: ShaderId = ShaderId(3);
const SECOND: TextureId = TextureId(11);
const THIRD: TextureId = TextureId(12);
const DEPTH: TextureId = TextureId(13);
const COMBINED: TextureId = TextureId(14);
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
/// The depth of the quad the fixture draws.
const QUAD_DEPTH: f32 = 0.5;

fn at(texture: TextureId) -> Attachment {
    Attachment { texture, mip: 0 }
}

/// The three colour textures and the depth-only one that follows them.
fn three_and_depth() -> Attachments {
    Attachments {
        colors: vec![at(TARGET), at(SECOND), at(THIRD)],
        depth: Some(at(DEPTH)),
    }
}

fn clear_all(color: [f64; 4]) -> Clear {
    Clear {
        color: Some(color),
        depth: Some(1.0),
        stencil: None,
    }
}

fn item(renderer: u64) -> ListDraw {
    ListDraw {
        renderer: RendererId(renderer),
        slot: 0,
        material: None,
        pass: alpha(),
        instances: Instances::Count(1),
        per_draw: PropertyLayer::default(),
    }
}

fn list(target: Attachments, draws: Vec<ListDraw>) -> Record {
    Record::Draws(Arc::new(DrawList {
        raster: DrawRaster::default(),
        target,
        values: PropertyLayer::default(),
        front_face: wgpu::FrontFace::Cw,
        draws,
    }))
}

fn register(rig: &mut Rig, id: ShaderId, synthetic: SyntheticShader) {
    rig.state.add_shader(
        id,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
}

/// The fixture's frame with its two renderers (red tint on the left half, blue on the right) drawn
/// by the shader with three outputs, three colour textures and a depth-only one.
fn frame_of(rig: &mut Rig, records: Vec<Record>) -> OffscreenFrame {
    let mut frame = fixture(rig, 0);
    register(rig, THREE, camera_pass_three_outputs());
    register(rig, COMBINING, camera_pass_combining());
    for material in frame.materials.values_mut() {
        material.shader = Some(THREE);
    }
    for id in [11, 12, 14] {
        frame.render_textures.push(texture(id, [8, 8]));
    }
    let mut depth = texture(13, [8, 8]);
    depth.graphics_format = 0;
    depth.depth_bits = 32;
    frame.render_textures.push(depth);
    frame.records = records;
    frame
}

fn read(rig: &mut Rig, frame: &OffscreenFrame, textures: &[TextureId]) -> Vec<Vec<u8>> {
    let reads: Vec<_> = textures.iter().map(|id| (*id, 0, 0)).collect();
    rig.run(frame, &reads).unwrap()
}

fn halves(image: &[u8], left: [u8; 4], right: [u8; 4], what: &str) {
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(pixel(image, 8, (x, y)), left, "{what}, left ({x}, {y})");
            assert_eq!(
                pixel(image, 8, (x + 4, y)),
                right,
                "{what}, right ({x}, {y})"
            );
        }
    }
}

fn depths(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks(4)
        .map(|float| f32::from_le_bytes(float.try_into().unwrap()))
        .collect()
}

#[test]
fn each_output_of_a_shader_reaches_its_own_texture_and_the_depth_reaches_another() {
    let mut rig = Rig::new();
    let target = three_and_depth();
    let records = vec![
        Record::Clear {
            target: target.clone(),
            clear: clear_all(CLEAR.map(|byte| f64::from(byte) / 255.0)),
        },
        list(target, vec![item(1)]),
    ];
    let frame = frame_of(&mut rig, records);
    let images = read(&mut rig, &frame, &[TARGET, SECOND, THIRD, DEPTH]);
    halves(&images[0], RED, CLEAR, "output 0");
    halves(&images[1], GREEN, CLEAR, "output 1");
    halves(&images[2], BLUE, CLEAR, "output 2");
    let depth = depths(&images[3]);
    for (index, value) in depth.iter().enumerate() {
        let expected = if index % 8 < 4 { QUAD_DEPTH } else { 0.0 };
        assert_eq!(value.to_bits(), expected.to_bits(), "depth texel {index}");
    }
}

#[test]
fn three_colours_draw_with_the_24_bit_depth_of_one_of_them_which_has_a_stencil_aspect() {
    let mut rig = Rig::new();
    let target = Attachments {
        colors: vec![at(TARGET), at(SECOND), at(THIRD)],
        depth: Some(at(TARGET)),
    };
    let records = vec![
        Record::Clear {
            target: target.clone(),
            clear: clear_all(CLEAR.map(|byte| f64::from(byte) / 255.0)),
        },
        list(target, vec![item(1)]),
    ];
    let mut frame = frame_of(&mut rig, records);
    for texture in &mut frame.render_textures {
        if texture.id == TARGET {
            texture.depth_bits = 24;
        }
    }
    let images = read(&mut rig, &frame, &[TARGET, SECOND, THIRD]);
    halves(&images[0], RED, CLEAR, "output 0");
    halves(&images[1], GREEN, CLEAR, "output 1");
    halves(&images[2], BLUE, CLEAR, "output 2");
}

#[test]
fn a_clear_sets_every_colour_attachment() {
    let mut rig = Rig::new();
    let records = vec![Record::Clear {
        target: three_and_depth(),
        clear: clear_all([1.0, 0.0, 0.0, 1.0]),
    }];
    let frame = frame_of(&mut rig, records);
    let images = read(&mut rig, &frame, &[TARGET, SECOND, THIRD]);
    for (number, image) in images.iter().enumerate() {
        assert!(
            image.chunks(4).all(|texel| texel == RED),
            "colour attachment {number} is cleared"
        );
    }
}

#[test]
fn a_later_draw_samples_two_of_the_outputs_it_chose_to() {
    let mut rig = Rig::new();
    let target = three_and_depth();
    let mut frame = frame_of(&mut rig, Vec::new());
    let mut combining = frame.materials[&MaterialId(1)].clone();
    combining.shader = Some(COMBINING);
    combining.properties = super::rig::layer(&[("_Tint", [1.0; 4])]);
    for (name, texture) in [("_First", TARGET), ("_Second", THIRD)] {
        combining.properties.textures.insert(
            name.to_owned(),
            TextureInput {
                texture,
                sampler: None,
            },
        );
    }
    frame.materials.insert(MaterialId(3), combining);
    let mut whole = frame.renderers[&RendererId(1)].clone();
    whole.materials = vec![MaterialId(3)];
    whole.per_draw = super::rig::layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    frame.renderers.insert(RendererId(3), whole);
    frame.records = vec![
        Record::Clear {
            target: target.clone(),
            clear: clear_all(CLEAR.map(|byte| f64::from(byte) / 255.0)),
        },
        list(target, vec![item(1)]),
        list(Attachments::single(Some(at(COMBINED)), None), vec![item(3)]),
    ];
    let images = read(&mut rig, &frame, &[COMBINED]);
    // Output 0 is red and output 2 blue on the left, and both are the clear color on the right.
    halves(&images[0], MAGENTA, CLEAR, "the combined texture");
}

#[test]
fn a_later_read_of_an_attachment_sees_the_pass_that_wrote_it_last() {
    let mut rig = Rig::new();
    let target = three_and_depth();
    let mut frame = frame_of(&mut rig, Vec::new());
    let mut combining = frame.materials[&MaterialId(1)].clone();
    combining.shader = Some(COMBINING);
    combining.properties = super::rig::layer(&[("_Tint", [1.0; 4])]);
    for (name, texture) in [("_First", TARGET), ("_Second", THIRD)] {
        combining.properties.textures.insert(
            name.to_owned(),
            TextureInput {
                texture,
                sampler: None,
            },
        );
    }
    frame.materials.insert(MaterialId(3), combining);
    let mut whole = frame.renderers[&RendererId(1)].clone();
    whole.materials = vec![MaterialId(3)];
    whole.per_draw = super::rig::layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    frame.renderers.insert(RendererId(3), whole);
    // Both renderers draw all three outputs, the right one with a blue tint. A pass over the
    // third texture alone then clears it to green, so the combining draw must read the first
    // texture as the draw wrote it and the third as that clear left it.
    let third_alone = Attachments::single(Some(at(THIRD)), None);
    frame.records = vec![
        Record::Clear {
            target: target.clone(),
            clear: clear_all(CLEAR.map(|byte| f64::from(byte) / 255.0)),
        },
        list(target.clone(), vec![item(1), item(2)]),
        Record::Clear {
            target: third_alone,
            clear: Clear {
                color: Some([0.0, 1.0, 0.0, 1.0]),
                depth: None,
                stencil: None,
            },
        },
        list(Attachments::single(Some(at(COMBINED)), None), vec![item(3)]),
    ];
    let images = read(&mut rig, &frame, &[COMBINED, THIRD]);
    assert!(
        images[1].chunks(4).all(|texel| texel == GREEN),
        "the third texture was cleared by the last pass that wrote it"
    );
    // The first texture is red on the left and blue on the right, the third green everywhere:
    // red and green give yellow, and blue and green give green.
    halves(
        &images[0],
        [255, 255, 0, 255],
        GREEN,
        "the combined texture",
    );
}

#[test]
fn a_shader_with_fewer_outputs_leaves_the_other_attachments_as_they_were() {
    let mut rig = Rig::new();
    let target = three_and_depth();
    let mut frame = frame_of(&mut rig, Vec::new());
    for material in frame.materials.values_mut() {
        material.shader = Some(super::camera::SHADER);
    }
    frame.records = vec![
        Record::Clear {
            target: target.clone(),
            clear: clear_all([0.0, 0.0, 1.0, 1.0]),
        },
        list(target, vec![item(1)]),
    ];
    let images = read(&mut rig, &frame, &[TARGET, SECOND, THIRD]);
    halves(&images[0], RED, BLUE, "the attachment the shader writes");
    for (number, image) in images[1..].iter().enumerate() {
        assert!(
            image.chunks(4).all(|texel| texel == BLUE),
            "attachment {} keeps its clear",
            number + 1
        );
    }
}

#[test]
fn a_shader_with_no_colour_output_leaves_every_attachment_as_it_was() {
    let mut rig = Rig::new();
    let target = Attachments::single(Some(at(TARGET)), None);
    let mut frame = frame_of(&mut rig, Vec::new());
    let silent = TINT.replace(
        "@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return material.tint; }",
        "@fragment fn fs_main(in: VOut) { if material.tint.r < -1.0 { discard; } }",
    );
    register(
        &mut rig,
        ShaderId(4),
        camera_pass_reading(&silent, &POSITION_AND_UV),
    );
    for material in frame.materials.values_mut() {
        material.shader = Some(ShaderId(4));
    }
    frame.records = vec![
        Record::Clear {
            target: target.clone(),
            clear: clear_all([0.0, 0.0, 1.0, 1.0]),
        },
        list(target, vec![item(1)]),
    ];
    let images = read(&mut rig, &frame, &[TARGET]);
    assert!(
        images[0].chunks(4).all(|texel| texel == BLUE),
        "the clear stays"
    );
}

#[test]
fn a_shader_with_more_outputs_than_attachments_writes_the_ones_there_are() {
    let mut rig = Rig::new();
    let target = Attachments::single(Some(at(TARGET)), None);
    let mut frame = frame_of(&mut rig, Vec::new());
    frame.records = vec![list(target, vec![item(1)])];
    let images = read(&mut rig, &frame, &[TARGET]);
    halves(&images[0], RED, ZERO, "output 0");
}

#[test]
fn more_than_eight_colours_are_refused_by_name() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    let mut colors = Vec::new();
    for id in 20..29 {
        frame.render_textures.push(texture(id, [8, 8]));
        colors.push(at(TextureId(id)));
    }
    frame.records = vec![list(
        Attachments {
            colors,
            depth: None,
        },
        vec![item(1)],
    )];
    let error = rig.build(&frame).err().expect("nine colours are refused");
    assert!(
        matches!(error, OffscreenError::ColorTargets { count: 9 }),
        "{error}"
    );
}

#[test]
fn colours_of_different_sizes_are_refused_by_name() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    frame.render_textures[1] = texture(11, [4, 4]);
    frame.records = vec![list(three_and_depth(), vec![item(1)])];
    let error = rig.build(&frame).err().expect("sizes differ");
    assert!(
        matches!(error, OffscreenError::ColorTargetsDiffer { .. }),
        "{error}"
    );
}

#[test]
fn one_texture_twice_as_a_colour_is_refused_by_name() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    frame.records = vec![list(
        Attachments {
            colors: vec![at(TARGET), at(TARGET)],
            depth: None,
        },
        vec![item(1)],
    )];
    let error = rig.build(&frame).err().expect("one texture twice");
    assert!(
        matches!(error, OffscreenError::ColorTargetRepeated { .. }),
        "{error}"
    );
}
