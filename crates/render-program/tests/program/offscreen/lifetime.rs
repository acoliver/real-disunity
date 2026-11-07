//! Render textures over frames: re-creation, the budget, and cancelled frames.

use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;

use super::camera::fixture;
use super::rig::{RED, Rig, ZERO, pixel};

const TARGET: TextureId = TextureId(10);

/// Runs `frame` and waits, then frees what the finished work held.
fn settle(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
    let image = rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    rig.runtime.collect();
    image
}

#[test]
fn a_render_texture_keeps_its_contents_between_frames_while_its_spec_is_the_same() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    let first = settle(&mut rig, &frame);
    assert_eq!(pixel(&first, 8, (0, 0)), RED);
    // No records: nothing draws, and the texture still holds the camera's pixels.
    frame.records.clear();
    let second = settle(&mut rig, &frame);
    assert_eq!(second, first);
}

#[test]
fn a_new_generation_creates_the_texture_again_and_releases_the_old_one() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    settle(&mut rig, &frame);
    let live = rig.runtime.live_bytes();
    frame.records.clear();
    frame.render_textures[0].generation = 1;
    let image = settle(&mut rig, &frame);
    assert!(
        image.chunks(4).all(|texel| texel == ZERO),
        "a new texture holds zeros"
    );
    // One more frame lets the old storage, which the previous frame may have used, be collected.
    settle(&mut rig, &frame);
    assert_eq!(
        rig.runtime.live_bytes(),
        live,
        "the replaced texture is no longer counted"
    );
}

#[test]
fn a_new_size_creates_a_texture_of_that_size() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    settle(&mut rig, &frame);
    frame.records.clear();
    frame.render_textures[0].size = [4, 2];
    let image = settle(&mut rig, &frame);
    assert_eq!(image.len(), 4 * 2 * 4);
    assert!(image.chunks(4).all(|texel| texel == ZERO));
}

#[test]
fn a_texture_the_budget_cannot_hold_is_a_named_error_and_nothing_is_created() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    let live = rig.runtime.live_bytes();
    // 4096 x 4096 x 4 bytes is 64 MiB, the whole budget, and the quad mesh is already in it.
    frame.render_textures[0].size = [4096, 4096];
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(
            error,
            OffscreenError::Program(ProgramError::OverBudget { .. })
        ),
        "{error}"
    );
    assert_eq!(rig.runtime.live_bytes(), live);
}

#[test]
fn a_cancelled_frame_leaves_the_target_the_counters_and_the_arena_as_they_were() {
    let mut rig = Rig::new();
    let frame = fixture(&mut rig, 1 << 1);
    let before = settle(&mut rig, &frame);
    let (live, serial) = (
        rig.runtime.live_bytes(),
        rig.gpu.authority.submitted_serial(),
    );
    // A different camera would change every pixel; the frame is built, admitted and cancelled.
    let mut other = fixture(&mut rig, 1 << 2);
    other
        .cameras
        .values_mut()
        .for_each(|camera| camera.clear_color = Some([0.0, 1.0, 0.0, 1.0]));
    let operations = rig.build(&other).unwrap();
    let authority = rig.gpu.authority.clone();
    let mut encoder = authority.begin(&rig.gpu.device).unwrap();
    let admitted = operations.admit(&encoder).unwrap();
    admitted.encode(None, &mut encoder).unwrap();
    // The frame holds the arena while it is being recorded.
    assert_eq!(
        rig.arena.retire().unwrap_err(),
        ProgramError::Lease(LeaseError::Busy)
    );
    encoder.finish().cancel();
    drop(admitted);
    rig.runtime.collect();
    assert_eq!(
        rig.gpu.authority.submitted_serial(),
        serial,
        "nothing was submitted"
    );
    assert_eq!(rig.runtime.live_bytes(), live, "the counters did not move");
    let after = settle(&mut rig, &frame);
    assert_eq!(after, before, "the target holds what it did");
    assert_ne!(pixel(&after, 8, (4, 4)), [0, 255, 0, 255]);
    // The cancelled frame released the arena, so it can retire.
    rig.arena.retire().unwrap();
}
