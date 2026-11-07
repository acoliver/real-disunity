//! The constants of a frame's draws: draws whose packed bytes are equal share one slice, and a
//! frame whose constants do not fit one buffer is split over several that draw the same.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, RendererId};
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::unity::offscreen::{OffscreenError, OffscreenOperations};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::camera_passes;

use super::camera::{CAMERA, SHADER, TARGET, camera, material, renderer};
use super::rig::{BLUE, CYAN, GREEN, QUAD, RED, Rig, pixel, quad, texture};
use worldrunner_render_program::unity::offscreen::frame::Record;

/// The corner of the 8x8 target each quadrant renderer covers, as `_Place`, and the color its
/// material tints it.
const QUADRANTS: [([f32; 4], [f32; 4], [u8; 4]); 4] = [
    ([0.5, 0.5, -0.5, -0.5], [1.0, 0.0, 0.0, 1.0], RED),
    ([0.5, 0.5, 0.5, -0.5], [0.0, 1.0, 0.0, 1.0], GREEN),
    ([0.5, 0.5, -0.5, 0.5], [0.0, 0.0, 1.0, 1.0], BLUE),
    ([0.5, 0.5, 0.5, 0.5], [0.0, 1.0, 1.0, 1.0], CYAN),
];

/// The top-left texel of each quadrant, in the order of `QUADRANTS`.
const CORNERS: [(u32, u32); 4] = [(0, 0), (4, 0), (0, 4), (4, 4)];

fn rig() -> Rig {
    let mut rig = Rig::new();
    let synthetic = camera_passes();
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.5))
        .unwrap();
    rig
}

/// One renderer for each quadrant, renderer `n` on layer `n + 1` with a material of its own.
fn quadrants() -> OffscreenFrame {
    let mut materials = BTreeMap::new();
    let mut renderers = BTreeMap::new();
    for (index, (place, tint, _)) in QUADRANTS.iter().enumerate() {
        let layer = u32::try_from(index + 1).unwrap();
        let id = u64::from(layer);
        materials.insert(MaterialId(id), material(*tint, 2000));
        renderers.insert(RendererId(id), renderer(id, layer, *place));
    }
    OffscreenFrame {
        render_textures: vec![texture(10, [8, 8])],
        materials,
        renderers,
        cameras: BTreeMap::from([(CAMERA, camera(0b11110))]),
        records: vec![Record::CameraRender {
            camera: CAMERA,
            target: TARGET,
        }],
        ..OffscreenFrame::default()
    }
}

fn build(rig: &mut Rig, frame: &OffscreenFrame) -> OffscreenOperations {
    rig.build(frame).unwrap()
}

fn render(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
    rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

#[test]
fn draws_that_pack_equal_constants_share_one_slice() {
    let mut rig = rig();
    let mut frame = quadrants();
    // Every renderer takes the first one's place and material.
    for id in 2..=4 {
        let first = frame.renderers[&RendererId(1)].clone();
        frame.renderers.insert(RendererId(id), first);
    }
    let shared = build(&mut rig, &frame).constants_bytes();
    frame.renderers.retain(|id, _| *id == RendererId(1));
    let one = build(&mut rig, &frame).constants_bytes();
    assert_eq!(shared, one, "four equal draws take the room of one");
}

#[test]
fn draws_that_differ_in_one_block_share_the_other() {
    let mut rig = rig();
    let mut frame = quadrants();
    // Both renderers use the first material and differ in place: the material block is shared and
    // each place has its own block at the next uniform offset.
    let alignment = u64::from(rig.gpu.device.limits().min_uniform_buffer_offset_alignment);
    frame.renderers.retain(|id, _| id.0 <= 2);
    frame.renderers.get_mut(&RendererId(2)).unwrap().materials = vec![MaterialId(1)];
    let bytes = build(&mut rig, &frame).constants_bytes();
    // Material (16 bytes), then the two places (16 bytes each), each starting at an alignment.
    assert_eq!(bytes, alignment.max(16) * 2 + 16);
}

#[test]
fn a_frame_whose_constants_exceed_one_buffer_draws_what_the_frames_split_by_hand_draw() {
    let mut rig = rig();
    let frame = quadrants();
    // A buffer this size holds one 16-byte block, so the two blocks of a draw are in two buffers.
    let alignment = u64::from(rig.gpu.device.limits().min_uniform_buffer_offset_alignment);
    rig.state.set_constants_limit(alignment.max(16));
    let operations = build(&mut rig, &frame);
    assert_eq!(
        operations.constants_buffers(),
        8,
        "one buffer for each block"
    );
    drop(operations);
    let split = render(&mut rig, &frame);
    for ((_, _, color), corner) in QUADRANTS.iter().zip(CORNERS) {
        for (dx, dy) in [(0, 0), (3, 3), (1, 2)] {
            let at = (corner.0 + dx, corner.1 + dy);
            assert_eq!(pixel(&split, 8, at), *color, "split frame at {at:?}");
        }
    }
    // Each renderer drawn alone, which needs one buffer, gives the pixels of its quadrant.
    for (index, corner) in CORNERS.iter().enumerate() {
        let mut alone = quadrants();
        alone.cameras.get_mut(&CAMERA).unwrap().culling_mask = 1 << (index + 1);
        let hand = render(&mut rig, &alone);
        for dy in 0..4 {
            for dx in 0..4 {
                let at = (corner.0 + dx, corner.1 + dy);
                assert_eq!(pixel(&split, 8, at), pixel(&hand, 8, at), "{at:?}");
            }
        }
    }
    // And the frame drawn with room for every draw in one buffer is the same image.
    let mut roomy = self::rig();
    assert_eq!(build(&mut roomy, &frame).constants_buffers(), 1);
    assert_eq!(render(&mut roomy, &frame), split);
}

#[test]
fn the_report_counts_the_blocks_packed_the_ones_shared_and_the_largest_buffer() {
    let mut rig = rig();
    let mut frame = quadrants();
    for id in 2..=4 {
        let first = frame.renderers[&RendererId(1)].clone();
        frame.renderers.insert(RendererId(id), first);
    }
    let operations = build(&mut rig, &frame);
    let used = operations.constants_use();
    // Four draws of two blocks each; only the first draw's two are new.
    assert_eq!((used.blocks, used.shared), (8, 6));
    assert_eq!(
        (used.buffers, used.bytes),
        (1, operations.constants_bytes())
    );
    assert_eq!(used.largest, operations.constants_bytes());
}

#[test]
fn constants_that_fit_the_buffer_are_one_upload() {
    let mut rig = rig();
    let operations = build(&mut rig, &quadrants());
    assert_eq!(operations.constants_buffers(), 1);
}

#[test]
fn a_block_larger_than_any_buffer_is_refused_by_its_size() {
    let mut rig = rig();
    rig.state.set_constants_limit(8);
    let Err(error) = rig.build(&quadrants()) else {
        panic!("a 16-byte block cannot fit a buffer of 8 bytes");
    };
    assert!(
        matches!(
            error,
            OffscreenError::ConstantsTooLarge { len: 16, limit: 8 }
        ),
        "{error}"
    );
}

#[test]
fn constants_past_the_most_a_frame_may_hold_are_refused_by_their_sizes() {
    let mut rig = rig();
    // A buffer holds one block; three blocks of 16 bytes pass 40 bytes at the third.
    let alignment = u64::from(rig.gpu.device.limits().min_uniform_buffer_offset_alignment);
    rig.state.set_constants_limit(alignment.max(16));
    rig.state.set_constants_total_limit(40);
    let Err(error) = rig.build(&quadrants()) else {
        panic!("three blocks of 16 bytes do not fit 40");
    };
    assert!(
        matches!(
            error,
            OffscreenError::ConstantsOverBudget {
                needed: 48,
                budget: 40
            }
        ),
        "{error}"
    );
    assert!(error.is_frame_wide());
}
