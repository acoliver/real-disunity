//! A shadow-map read: a camera writes depth into a depth-only render texture and a later blit
//! compares a reference depth with it through a comparison sampler.
//!
//! The renderer's depth is reversed (1 is near, 0 is far, the camera clears to 0 and a pass's
//! `ZTest LEqual` is `GreaterEqual`), so a receiver is lit when its reference depth is at least the
//! stored depth. The shader writes the comparison result as colour, so the expected image is a
//! table of ones and zeros computed here from the depths.

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{blit_quad, shadow_blit};

use super::depth_only::depth_frame;
use super::rig::{BLIT_QUAD, Rig, image, pixel, source, texture};

const SHADOW_SHADER: ShaderId = ShaderId(2);
const SHADOW_MAP: TextureId = TextureId(10);
const LIT_MAP: TextureId = TextureId(11);
const SOURCE: TextureId = TextureId(5);
const SIDE: u32 = 8;
const LEFT_ONLY: u32 = 1 << 1;
const BOTH: u32 = (1 << 1) | (1 << 2);

/// The depths `depth_frame` draws with `BOTH`: the left half at 0.5 and the right half at 0.25.
const LEFT_DEPTH: f32 = 0.5;
const RIGHT_DEPTH: f32 = 0.25;
/// What the camera clears to where it draws nothing.
const FAR: f32 = 0.0;
/// Between the two drawn depths, so the receiver is behind the left half and in front of the
/// right half.
const REFERENCE: f32 = 0.375;

/// A frame that draws the shadow map with `depth_frame` and then blits `source` through the
/// comparison shader into an 8x8 colour texture.
fn shadow_frame(rig: &mut Rig, mask: u32, source: TextureId) -> OffscreenFrame {
    let mut frame = depth_frame(rig, mask, 32);
    let synthetic = shadow_blit("_MainTex", REFERENCE);
    rig.state.add_shader(
        SHADOW_SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    frame.blit_mesh = Some(BLIT_QUAD);
    frame.render_textures.push(texture(11, [SIDE, SIDE]));
    frame.materials.insert(
        MaterialId(3),
        Material {
            shader: Some(SHADOW_SHADER),
            keywords: Vec::new(),
            properties: PropertyLayer::default(),
            fallback: PropertyLayer::default(),
            below_globals: PropertyLayer::default(),
            render_queue: 2000,
        },
    );
    frame.records.push(Record::Blit {
        source,
        target: LIT_MAP,
        material: Some(MaterialId(3)),
        pass: 0,
    });
    frame
}

fn lit(stored: f32) -> u8 {
    if REFERENCE >= stored { 255 } else { 0 }
}

fn assert_columns(image: &[u8], left: f32, right: f32) {
    for y in 0..SIDE {
        for x in 0..SIDE {
            let stored = if x < SIDE / 2 { left } else { right };
            let expected = lit(stored);
            let [red, green, blue, alpha] = pixel(image, SIDE, (x, y));
            for channel in [red, green, blue] {
                assert!(
                    channel.abs_diff(expected) <= 1,
                    "({x}, {y}) is {channel}, expected {expected} for the stored depth {stored}"
                );
            }
            assert_eq!(alpha, 255, "({x}, {y})");
        }
    }
}

#[test]
fn a_comparison_sampler_reads_one_where_the_receiver_is_in_front_of_the_stored_depth_and_zero_behind_it()
 {
    let mut rig = Rig::new();
    let frame = shadow_frame(&mut rig, BOTH, SHADOW_MAP);
    let image = rig.run(&frame, &[(LIT_MAP, 0, 0)]).unwrap().remove(0);
    // The receiver at 0.375 is behind the left half's 0.5 and in front of the right half's 0.25.
    assert_eq!(lit(LEFT_DEPTH), 0);
    assert_eq!(lit(RIGHT_DEPTH), 255);
    assert_columns(&image, LEFT_DEPTH, RIGHT_DEPTH);
}

#[test]
fn a_receiver_over_what_the_camera_never_drew_is_lit_by_the_cleared_far_depth() {
    let mut rig = Rig::new();
    let frame = shadow_frame(&mut rig, LEFT_ONLY, SHADOW_MAP);
    let image = rig.run(&frame, &[(LIT_MAP, 0, 0)]).unwrap().remove(0);
    assert_columns(&image, LEFT_DEPTH, FAR);
}

#[test]
fn a_second_frame_of_the_same_draw_reads_what_the_first_did() {
    let mut rig = Rig::new();
    let frame = shadow_frame(&mut rig, BOTH, SHADOW_MAP);
    let first = rig.run(&frame, &[(LIT_MAP, 0, 0)]).unwrap().remove(0);
    let second = rig.run(&frame, &[(LIT_MAP, 0, 0)]).unwrap().remove(0);
    assert_eq!(first, second);
    assert_columns(&second, LEFT_DEPTH, RIGHT_DEPTH);
}

#[test]
fn a_comparison_sampler_over_a_colour_texture_is_refused_by_name() {
    let mut rig = Rig::new();
    let mut frame = shadow_frame(&mut rig, BOTH, SOURCE);
    rig.state
        .set_texture(
            &mut rig.runtime,
            &source(5, [4, 4], image([4, 4], |_, _| [255, 0, 0, 255])),
        )
        .unwrap();
    // The colour source takes the depth read's place, so the camera's work is not needed.
    frame
        .records
        .retain(|record| matches!(record, Record::Blit { .. }));
    let error = rig
        .build(&frame)
        .err()
        .expect("a colour texture has no depth");
    assert!(
        matches!(
            error,
            OffscreenError::UnsupportedBinding { set: 0, binding: 0, what }
                if what.contains("depth")
        ),
        "{error}"
    );
}

#[test]
fn the_draw_builds_where_the_runtime_once_refused_the_comparison_sampler() {
    let mut rig = Rig::new();
    let frame = shadow_frame(&mut rig, BOTH, SHADOW_MAP);
    let built = rig.build(&frame);
    assert!(built.is_ok(), "{:?}", built.err());
}
