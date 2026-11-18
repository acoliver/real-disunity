//! The committed gather mirror changes only when a published frame changed it.
//!
//! A moving view over unchanged placements finds every batch range already holding its content,
//! so it publishes the mirror it found. A transaction that is cancelled, whether it moved the view
//! only, edited a batch, or grew the mirror, leaves the committed mirror exactly as it was.
use super::compaction::{Rig, cards, pose_at, unlit};

fn edited(rig: &mut Rig) {
    let handle = rig.handles()[1];
    let mut transform = *super::compaction::placement(1).transform();
    transform.position_millimeters[0] -= 1_000;
    rig.gpu.set_instance_transform(handle, transform).unwrap();
}

fn grown(rig: &mut Rig) {
    let template = super::compaction::placement(0);
    for _ in 0..40 {
        rig.gpu.add_instance(template, true).unwrap();
    }
}

#[test]
fn a_moving_view_over_unchanged_placements_leaves_the_committed_mirror_unchanged() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&cards(), false);
    rig.render(&unlit(pose_at(0)));
    let settled = rig.gpu.committed_mirror();
    assert!(settled.batches() >= 2, "{settled:?}");
    for x in [1_000, 2_000, -1_500] {
        let gathered = rig.gpu.views[0].compaction.observed.staged.get();
        rig.render(&unlit(pose_at(x)));
        assert!(
            rig.gpu.views[0].compaction.observed.staged.get() > gathered,
            "the moved view gathered again"
        );
        assert_eq!(rig.gpu.committed_mirror(), settled);
    }
}

#[test]
fn a_cancelled_transaction_leaves_the_committed_mirror_exact() {
    for change in [None, Some(edited as fn(&mut Rig)), Some(grown)] {
        let _session = crate::test_support::SceneTestSession::acquire();
        let mut rig = Rig::over(&cards(), false);
        rig.render(&unlit(pose_at(0)));
        let committed = rig.gpu.committed_mirror();
        if let Some(change) = change {
            change(&mut rig);
        }
        rig.prepare(&unlit(pose_at(2_000))).cancel();
        assert_eq!(rig.gpu.committed_mirror(), committed);
        // The frame that does publish brings the mirror up to what the scene now holds.
        let published = rig.render(&unlit(pose_at(2_000)));
        let mut ranged = Rig::over(&cards(), true);
        if let Some(change) = change {
            change(&mut ranged);
        }
        assert_eq!(
            published.pixels,
            ranged.render(&unlit(pose_at(2_000))).pixels
        );
        assert_eq!(change.is_some(), rig.gpu.committed_mirror() != committed);
    }
}

#[test]
fn a_moving_view_over_unchanged_placements_checks_the_mirror_ranges_once() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&cards(), false);
    rig.render(&unlit(pose_at(0)));
    let settled = rig.gpu.mirror_scans();
    assert!(settled > 0, "the first frame filled the mirror");
    for x in [1_000, 2_000, -1_500, 0] {
        rig.render(&unlit(pose_at(x)));
        assert_eq!(rig.gpu.mirror_scans(), settled, "the moved view rechecked");
    }
}

#[test]
fn an_edit_makes_the_next_frame_check_and_copy_the_mirror_ranges_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&cards(), false);
    rig.render(&unlit(pose_at(0)));
    rig.render(&unlit(pose_at(1_000)));
    let (committed, settled) = (rig.gpu.committed_mirror(), rig.gpu.mirror_scans());
    edited(&mut rig);
    rig.render(&unlit(pose_at(1_000)));
    let rechecked = rig.gpu.mirror_scans();
    assert!(rechecked > settled, "the edit went unnoticed");
    assert_ne!(rig.gpu.committed_mirror(), committed);
    rig.render(&unlit(pose_at(2_000)));
    assert_eq!(rig.gpu.mirror_scans(), rechecked);
}

#[test]
fn a_cancelled_edit_leaves_the_next_frame_to_check_the_mirror_ranges_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&cards(), false);
    rig.render(&unlit(pose_at(0)));
    rig.render(&unlit(pose_at(1_000)));
    let committed = rig.gpu.committed_mirror();
    edited(&mut rig);
    rig.prepare(&unlit(pose_at(1_000))).cancel();
    let cancelled = rig.gpu.mirror_scans();
    assert_eq!(rig.gpu.committed_mirror(), committed);
    rig.render(&unlit(pose_at(1_000)));
    assert!(
        rig.gpu.mirror_scans() > cancelled,
        "the edit went unnoticed"
    );
    assert_ne!(rig.gpu.committed_mirror(), committed);
}
