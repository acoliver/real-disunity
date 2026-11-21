//! The committed gather mirror changes only when a published frame changed it.
//!
//! A moving view over unchanged placements finds every batch range already holding its content,
//! so it publishes the mirror it found. A transaction that is cancelled, whether it moved the view
//! only, edited a batch, or grew the mirror, leaves the committed mirror exactly as it was.
use super::compaction::{Rig, cards, mesh, placement, pose_at, unlit};
use super::*;

/// The eight cards plus a ninth of a third material (a third batch) 14 m to the right, which the
/// camera at the origin does not see and the camera 7 m to the right does.
fn with_distant_card() -> RenderScene {
    let source = super::lod_material::scene(true);
    let mut transform = *placement(1).transform();
    transform.position_millimeters[0] = 14_000;
    let distant = SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(1),
    );
    let objects = (0..8).map(placement).chain([distant]).collect();
    source
        .with_resources(
            source.resources().clone(),
            SceneGeometry::new(vec![mesh()], objects).unwrap(),
            None,
        )
        .unwrap()
}

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

#[test]
fn a_batch_entering_the_view_is_the_only_batch_the_mirror_checks() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&with_distant_card(), false);
    let mut ranged = Rig::over(&with_distant_card(), true);
    for (step, x) in [0, 7_000, 0, 7_000, 3_000].into_iter().enumerate() {
        let (scans, checked) = (rig.gpu.mirror_scans(), rig.gpu.mirror_checked());
        let batches = rig.gpu.committed_mirror().batches();
        let published = rig.render(&unlit(pose_at(x)));
        assert_eq!(
            published.pixels,
            ranged.render(&unlit(pose_at(x))).pixels,
            "x {x}"
        );
        let entered = rig.gpu.committed_mirror().batches() - batches;
        match step {
            0 => assert_eq!((entered, rig.gpu.mirror_checked() - checked), (2, 2)),
            1 => {
                assert_eq!(entered, 1, "the distant card's batch enters");
                assert_eq!(rig.gpu.mirror_scans(), scans + 1);
                assert_eq!(rig.gpu.mirror_checked(), checked + 1, "only the new batch");
            }
            _ => assert_eq!((entered, rig.gpu.mirror_checked()), (0, checked), "x {x}"),
        }
    }
}

fn moved(rig: &mut Rig) {
    let handle = rig.handles()[1];
    let mut transform = *placement(1).transform();
    transform.position_millimeters[0] += 900;
    rig.gpu.set_instance_transform(handle, transform).unwrap();
}

fn appended(rig: &mut Rig) {
    // Outgrows the first batch's range, so the batch takes a new range at the end of the mirror.
    for _ in 0..40 {
        rig.gpu.add_instance(placement(0), true).unwrap();
    }
}

/// Moves card 2, of material 0, into the batch of material 2.
fn re_materialled(rig: &mut Rig) {
    let handle = rig.handles()[2];
    let (card, _) = rig.gpu.instance(handle).unwrap();
    let replaced = SceneObject::new(*card.transform(), card.mesh(), MaterialHandle::from_raw(2));
    rig.gpu.add_instance(replaced, true).unwrap();
    rig.gpu.remove_instance(handle).unwrap();
}

fn hidden(rig: &mut Rig) {
    let handle = rig.handles()[4];
    rig.gpu.set_instance_visible(handle, false).unwrap();
}

/// An edit to the scene a rig holds, and whether it rewrites instance bytes.
struct Change {
    name: &'static str,
    apply: fn(&mut Rig),
    rewrites_bytes: bool,
}

impl Change {
    fn new(name: &'static str, apply: fn(&mut Rig), rewrites_bytes: bool) -> Self {
        Self {
            name,
            apply,
            rewrites_bytes,
        }
    }
}

/// A change to one batch reaches every view that selects it, however much of the mirror an
/// earlier view had already found up to date: the pixels equal those of ranged drawing at both
/// the camera that sees two batches and the camera that sees three.
#[test]
fn a_change_to_a_batch_reaches_every_view_that_selects_it() {
    // Hiding a card changes what the views select and no instance bytes, so the mirror has
    // nothing to copy for it.
    let changes = [
        Change::new("moved", moved, true),
        Change::new("appended", appended, true),
        Change::new("re-materialled", re_materialled, true),
        Change::new("hidden", hidden, false),
    ];
    for Change {
        name,
        apply: change,
        rewrites_bytes,
    } in changes
    {
        let _session = crate::test_support::SceneTestSession::acquire();
        let mut rig = Rig::over(&with_distant_card(), false);
        let mut ranged = Rig::over(&with_distant_card(), true);
        for x in [0, 7_000, 0] {
            rig.render(&unlit(pose_at(x)));
        }
        let settled = (rig.gpu.mirror_scans(), rig.gpu.committed_mirror());
        change(&mut rig);
        change(&mut ranged);
        for x in [0, 7_000, 0, 7_000] {
            let published = rig.render(&unlit(pose_at(x)));
            let reference = ranged.render(&unlit(pose_at(x)));
            assert_eq!(published.pixels, reference.pixels, "{name} at {x}");
        }
        assert_eq!(rig.gpu.mirror_scans() > settled.0, rewrites_bytes, "{name}");
        assert_eq!(
            rig.gpu.committed_mirror() != settled.1,
            rewrites_bytes,
            "{name}"
        );
        // Once both cameras have found the change, neither checks anything again.
        let (scans, checked) = (rig.gpu.mirror_scans(), rig.gpu.mirror_checked());
        for x in [0, 7_000, 3_000, 0] {
            rig.render(&unlit(pose_at(x)));
        }
        assert_eq!(rig.gpu.mirror_scans(), scans, "{name}");
        assert_eq!(rig.gpu.mirror_checked(), checked, "{name}");
    }
}
