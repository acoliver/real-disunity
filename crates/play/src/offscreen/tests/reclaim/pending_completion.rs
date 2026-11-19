use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use worldrunner_render_gpu::test_support::HeldPoller;

#[test]
fn pending_completion_keeps_reclaimed_texture_charged_until_settled() {
    let base = base_bytes();
    let mut room = Room::new(base + TEXTURE_BYTES + 8192);
    room.submit_for_viewer(&blitting_from(A, 4));

    let live = room.runtime.live_bytes();
    let serial = room.device.1.submitted_serial();
    let b_only = naming(&[B], 4);
    // Hold completion bookkeeping after native work is done, before anything polls again.
    let held = HeldPoller::start(room.runtime.device());
    let pending = catch_unwind(AssertUnwindSafe(|| {
        assert_pending_reclaim(&mut room, live, serial, &b_only);
    }));

    let releaser = held.release_after(std::time::Duration::ZERO);
    releaser.join().unwrap();
    held.join();
    if let Err(panic) = pending {
        resume_unwind(panic);
    }

    assert!(room.device.1.settled_through(serial));
    room.runtime.collect();
    assert_eq!(room.runtime.live_bytes(), live - TEXTURE_BYTES);

    let stored = room.adapt_for_viewer(&b_only).report;
    assert_eq!(over_budget(&stored), 0, "{:?}", stored.causes);
    assert_eq!(stored.reclaimed, Reclaimed::default());
    assert!(
        room.state
            .texture_handle(ids::stored_texture(B).unwrap())
            .is_some()
    );
    assert_eq!(room.runtime.live_bytes(), live);
}

fn assert_pending_reclaim(room: &mut Room, live: u64, serial: u64, b_only: &GraphicsSnapshot) {
    assert!(serial > 0);
    assert!(room.device.1.completed_serial() < serial);
    assert!(!room.device.1.settled_through(serial));

    let refused = room.adapt_for_viewer(b_only).report;
    assert_eq!(over_budget(&refused), 1, "{:?}", refused.causes);
    assert_eq!(
        (
            refused.reclaimed.textures,
            refused.reclaimed.meshes,
            refused.reclaimed.targets,
            refused.reclaimed.bytes,
        ),
        (1, 0, 0, TEXTURE_BYTES)
    );
    assert!(
        room.state
            .texture_handle(ids::stored_texture(A).unwrap())
            .is_none()
    );
    assert!(
        room.state
            .texture_handle(ids::stored_texture(B).unwrap())
            .is_none()
    );
    assert_eq!(room.runtime.live_bytes(), live);
    assert!(!room.device.1.settled_through(serial));
}
