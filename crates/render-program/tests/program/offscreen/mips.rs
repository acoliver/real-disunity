//! A render texture with a mip chain that a camera wrote at its base level and a later draw
//! samples: the engine writes no other level, so the draw reads the levels written at the same
//! point as the base and the frame counts the read.

use std::collections::BTreeMap;
use worldrunner_render_program::layers::PropertyLayer;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::unity::offscreen::PartialMipReads;
use worldrunner_render_program::unity::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::blit_passes;

use super::camera::fixture;
use super::rig::{BLIT_QUAD, BLUE, CLEAR, RED, Rig, blit_quad, layer, pixel, texture};

const BLIT: ShaderId = ShaderId(2);
const CAMERA_TARGET: TextureId = TextureId(10);
const COPY: TextureId = TextureId(11);

/// The camera fixture (red left half over the clear color on an 8x8 target of `mips` levels),
/// then a blit of that target through the identity tint into an 8x8 target of its own.
fn camera_then_blit(rig: &mut Rig, mips: u32) -> OffscreenFrame {
    let mut frame = fixture(rig, 1 << 1);
    frame.render_textures[0].mips = mips;
    frame.render_textures.push(texture(11, [8, 8]));
    let synthetic = blit_passes("_MainTex");
    rig.state.add_shader(
        BLIT,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    frame.blit_mesh = Some(BLIT_QUAD);
    frame.materials.insert(
        MaterialId(3),
        Material {
            shader: Some(BLIT),
            keywords: Vec::new(),
            properties: layer(&[("_Tint", [1.0; 4])]),
            fallback: PropertyLayer::default(),
            render_queue: 2000,
        },
    );
    frame.records.push(Record::Blit {
        source: CAMERA_TARGET,
        target: COPY,
        material: Some(MaterialId(3)),
        pass: 0,
    });
    frame
}

fn halves() -> BTreeMap<&'static str, [u8; 4]> {
    BTreeMap::from([("left", RED), ("right", CLEAR)])
}

#[test]
fn a_draw_reads_the_base_level_of_a_render_texture_whose_other_levels_were_never_written() {
    let mut rig = Rig::new();
    let frame = camera_then_blit(&mut rig, 3);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.partial_mip_reads(), 1);
    drop(operations);
    let image = rig.run(&frame, &[(COPY, 0, 0)]).unwrap().remove(0);
    let expected = halves();
    for y in 0..8 {
        assert_eq!(pixel(&image, 8, (1, y)), expected["left"], "left ({y})");
        assert_eq!(pixel(&image, 8, (6, y)), expected["right"], "right ({y})");
    }
    assert_ne!(expected["left"], BLUE);
}

#[test]
fn a_render_texture_with_one_level_is_read_whole_and_counts_nothing() {
    let mut rig = Rig::new();
    let frame = camera_then_blit(&mut rig, 1);
    assert_eq!(rig.build(&frame).unwrap().partial_mip_reads(), 0);
}

#[test]
fn a_render_texture_nothing_has_written_is_read_whole_and_counts_nothing() {
    let mut rig = Rig::new();
    let mut frame = camera_then_blit(&mut rig, 3);
    // Only the blit remains, so every level of its source is still at its first version.
    frame.records.remove(0);
    assert_eq!(rig.build(&frame).unwrap().partial_mip_reads(), 0);
}

#[test]
fn partial_reads_are_listed_by_the_texture_property_the_levels_and_the_pass_that_read_them() {
    let mut rig = Rig::new();
    let frame = camera_then_blit(&mut rig, 3);
    assert_eq!(
        rig.build(&frame).unwrap().partial_mip_reads_by_pass(),
        vec![PartialMipReads {
            texture: "_MainTex".to_owned(),
            written: 1,
            total: 3,
            key: "index".to_owned(),
            tag: Some("0".to_owned()),
            reads: 1,
        }]
    );
}
