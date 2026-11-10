//! Render textures over frames: re-creation, the budget, and cancelled frames.

use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;

use super::camera::fixture;
use super::rig::{QUAD, RED, Rig, ZERO, pixel, quad, source};

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

#[test]
fn an_uploaded_texture_and_a_mesh_give_back_their_bytes_when_released_and_only_once() {
    let mut rig = Rig::new();
    let before = rig.runtime.live_bytes();
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.5))
        .unwrap();
    rig.state
        .set_texture(&mut rig.runtime, &source(20, [8, 8], vec![1; 8 * 8 * 4]))
        .unwrap();
    // The quad is four positions of 12 bytes, four texture coordinates of 8 and six 16-bit
    // indices; the texture is 8 x 8 texels of 4 bytes.
    let (mesh, texels) = (4 * 12 + 4 * 8 + 6 * 2, 8 * 8 * 4);
    assert_eq!(rig.runtime.live_bytes(), before + mesh + texels);

    assert_eq!(rig.state.release_mesh(&mut rig.runtime, QUAD), Some(mesh));
    assert_eq!(
        rig.state
            .release_source_texture(&mut rig.runtime, TextureId(20)),
        Some(texels)
    );
    rig.runtime.collect();
    assert_eq!(rig.runtime.live_bytes(), before);
    assert_eq!(rig.state.release_mesh(&mut rig.runtime, QUAD), None);
    assert_eq!(
        rig.state
            .release_source_texture(&mut rig.runtime, TextureId(20)),
        None
    );
}

#[test]
fn a_render_texture_is_not_released_as_an_uploaded_one() {
    let mut rig = Rig::new();
    let frame = fixture(&mut rig, 1 << 1);
    rig.build(&frame).unwrap();
    let live = rig.runtime.live_bytes();
    assert_eq!(
        rig.state.release_source_texture(&mut rig.runtime, TARGET),
        None
    );
    rig.runtime.collect();
    assert_eq!(rig.runtime.live_bytes(), live);
}

#[test]
fn the_storage_a_frames_render_textures_would_add_is_counted_before_they_are_made() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    // 8 x 8 texels of 4 bytes.
    let color = 8 * 8 * 4;
    let would_add = |rig: &Rig, frame: &OffscreenFrame| {
        rig.state
            .new_render_texture_bytes(&rig.runtime, &frame.render_textures)
    };
    assert_eq!(would_add(&rig, &frame), color);
    rig.build(&frame).unwrap();
    assert_eq!(would_add(&rig, &frame), 0, "the texture exists");
    // A 24-bit depth buffer takes 4 bytes a texel, and 4 levels take 64 + 16 + 4 + 1 texels.
    frame.render_textures[0].depth_bits = 24;
    frame.render_textures[0].mips = 4;
    assert_eq!(would_add(&rig, &frame), (64 + 16 + 4 + 1) * 4 + 8 * 8 * 4);
}

#[test]
fn the_render_textures_the_state_holds_are_listed_with_their_bytes_and_can_be_released_alone() {
    let mut rig = Rig::new();
    let frame = fixture(&mut rig, 1 << 1);
    let before = rig.runtime.live_bytes();
    let drawn = settle(&mut rig, &frame);
    assert_eq!(pixel(&drawn, 8, (0, 0)), RED);
    // An 8 x 8 target of 4-byte texels, and the constants buffer the camera's frame needs.
    let held = rig.state.render_textures_held();
    assert_eq!(held, [(TARGET, 8 * 8 * 4)]);
    let live = rig.runtime.live_bytes();
    assert!(live >= before + 8 * 8 * 4);

    let freed = rig.state.release_render_texture(&mut rig.runtime, TARGET);
    rig.runtime.collect();
    assert_eq!(freed, Some(8 * 8 * 4));
    assert_eq!(rig.runtime.live_bytes(), live - 8 * 8 * 4);
    assert!(rig.state.render_textures_held().is_empty());
    assert_eq!(
        rig.state.release_render_texture(&mut rig.runtime, TARGET),
        None,
        "a texture is given back once"
    );

    // The next frame that names the texture makes it again, empty.
    let mut again = frame.clone();
    again.records.clear();
    let image = settle(&mut rig, &again);
    assert!(image.chunks(4).all(|texel| texel == ZERO));
}

#[test]
fn an_uploaded_source_is_not_a_render_texture_to_release() {
    let mut rig = Rig::new();
    rig.state
        .set_texture(&mut rig.runtime, &source(20, [8, 8], vec![1; 8 * 8 * 4]))
        .unwrap();
    assert!(rig.state.render_textures_held().is_empty());
    assert_eq!(
        rig.state
            .release_render_texture(&mut rig.runtime, TextureId(20)),
        None
    );
}

#[test]
fn the_staging_a_read_of_a_render_texture_needs_is_its_rows_padded_to_the_copy_alignment() {
    let mut rig = Rig::new();
    let frame = fixture(&mut rig, 1 << 1);
    settle(&mut rig, &frame);
    // Eight texels of 4 bytes is 32 bytes a row, which the copy pads to 256, for eight rows.
    assert_eq!(rig.state.read_bytes(TARGET), Some(256 * 8));
    assert_eq!(rig.state.read_bytes(TextureId(99)), None);
}
