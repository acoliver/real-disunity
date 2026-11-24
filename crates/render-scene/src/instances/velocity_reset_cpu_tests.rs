use super::*;
use crate::instances::test_owner::Owner;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation};

fn candidate(batch: &mut InstanceBatch) -> super::super::history::Candidate {
    batch
        .history
        .prepare(&batch.bytes, &batch.dirty, &Owner::new())
}

fn previous_uploads(batch: &mut InstanceBatch) -> Vec<(u64, Vec<u8>)> {
    let candidate = candidate(batch);
    batch
        .history
        .previous_uploads(&candidate, &batch.bytes, false)
}

fn batch() -> InstanceBatch {
    filled(InstanceBatch::default())
}

fn skinned_batch() -> InstanceBatch {
    filled(InstanceBatch::skinned())
}

fn filled(mut batch: InstanceBatch) -> InstanceBatch {
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    batch.add(object, true);
    batch.add(object, true);
    batch.commit(&batch.pending_ranges());
    candidate(&mut batch).publish();
    batch.set_transform(
        0,
        SceneTransform {
            position_millimeters: [500, 0, 0],
            ..*object.transform()
        },
    );
    batch
}

#[test]
fn physical_acknowledgement_keeps_history_pending_without_current_edits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut batch = batch();
    let before = previous_uploads(&mut batch);
    batch.commit(&batch.pending_ranges());
    assert!(batch.pending_ranges().is_empty());
    assert!(batch.history.has_pending());
    assert_eq!(previous_uploads(&mut batch), before);
    let content = batch.content();
    let edits = batch.edits();
    candidate(&mut batch).publish();
    assert_eq!(batch.content(), content);
    assert_eq!(batch.edits(), edits);
    assert!(batch.history.has_pending());
    assert_eq!(
        previous_uploads(&mut batch),
        vec![(0, batch.bytes[..STRIDE].to_vec())]
    );
    candidate(&mut batch).publish();
    assert!(!batch.history.has_pending());
}

fn stage_current(batch: &mut InstanceBatch, owner: &Publications) {
    let chunks = batch
        .pending_ranges_for(Some(owner))
        .iter()
        .map(|range| {
            let range = byte_range(range);
            (range.clone(), batch.bytes[range].to_vec())
        })
        .collect::<Vec<_>>();
    batch.staged.record(owner, &chunks, batch.skin.as_ref());
}

#[test]
fn repeated_flush_cpu_restores_current_and_preserves_other_staged_slots() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut batch = batch();
    let a = batch.get(1).0;
    let b = batch.get(0).0;
    batch.set_object(1, b);
    let owner = Owner::new();
    assert_eq!(batch.pending_ranges_for(Some(&*owner)), vec![0..2]);
    stage_current(&mut batch, &owner);
    batch.set_object(0, a);
    batch.prune_published(&owner);
    assert_eq!(batch.pending_ranges_for(Some(&*owner)), vec![0..1]);
    stage_current(&mut batch, &owner);
    batch.prune_published(&owner);
    assert!(batch.pending_ranges_for(Some(&*owner)).is_empty());
    batch.set_object(0, b);
    assert_eq!(batch.pending_ranges_for(Some(&*owner)), vec![0..1]);
    assert_eq!(batch.pending_ranges(), vec![0..2]);
}

#[test]
fn repeated_flush_cpu_restores_skin_without_transform_edits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut batch = skinned_batch();
    for slot in 0..2 {
        batch.set_skin(slot, [0, 0, 2, 0]);
    }
    batch.commit(&batch.pending_ranges());
    let current = batch.bytes.clone();
    let owner = Owner::new();
    batch.set_skin(0, [4, 4, 2, 0]);
    stage_current(&mut batch, &owner);
    batch.set_skin(0, [0, 0, 2, 0]);
    batch.prune_published(&owner);
    assert_eq!(batch.pending_ranges_for(Some(&*owner)), vec![0..1]);
    stage_current(&mut batch, &owner);
    batch.prune_published(&owner);
    assert!(batch.pending_ranges_for(Some(&*owner)).is_empty());
    batch.set_skin(1, [8, 8, 2, 0]);
    assert_eq!(batch.pending_ranges_for(Some(&*owner)), vec![1..2]);
    assert_eq!(batch.bytes, current);
}

#[test]
fn repeated_flush_cpu_keeps_unsubmitted_edits_pending_outside_the_owner() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut batch = batch();
    let owner = Owner::new();
    stage_current(&mut batch, &owner);
    batch.prune_published(&owner);
    assert!(batch.pending_ranges_for(Some(&*owner)).is_empty());
    assert_eq!(batch.pending_ranges(), vec![0..1]);
    let other = Owner::new();
    batch.prune_published(&other);
    assert_eq!(batch.pending_ranges_for(Some(&*other)), vec![0..1]);
    stage_current(&mut batch, &other);
    assert!(batch.pending_ranges_for(Some(&*other)).is_empty());
    assert_eq!(batch.pending_ranges_for(Some(&*owner)), vec![0..1]);
    assert_eq!(batch.pending_ranges(), vec![0..1]);
}
