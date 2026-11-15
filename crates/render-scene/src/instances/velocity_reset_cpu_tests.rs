use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation};

fn candidate(batch: &mut InstanceBatch) -> super::super::history::Candidate {
    batch.history.prepare(
        &batch.bytes,
        &batch.dirty,
        &worldrunner_render_gpu::submission::Publications::default(),
    )
}

fn previous_uploads(batch: &mut InstanceBatch) -> Vec<(u64, Vec<u8>)> {
    let candidate = candidate(batch);
    batch
        .history
        .previous_uploads(&candidate, &batch.bytes, false)
}

fn batch() -> InstanceBatch {
    let mut batch = InstanceBatch::default();
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
fn velocity_reset_cpu_makes_all_previous_transforms_current_until_submission() {
    let mut batch = batch();
    let before = previous_uploads(&mut batch);
    assert_ne!(before[0].1, batch.bytes[..STRIDE]);
    batch.reset_velocity_history();
    for _ in 0..2 {
        let reset = previous_uploads(&mut batch);
        assert_eq!(reset, vec![(0, batch.bytes.clone())]);
        assert!(candidate(&mut batch).moving_slots().is_empty());
        assert!(batch.history.has_pending());
    }
}

#[test]
fn velocity_reset_cpu_ignores_removed_slots_and_keeps_current_uploads_sparse() {
    let mut batch = batch();
    batch.remove(1);
    let writes = batch.pending_ranges();
    batch.reset_velocity_history();
    assert_eq!(batch.pending_ranges(), writes);
    let candidate = candidate(&mut batch);
    assert_eq!(candidate.slots().collect::<Vec<_>>(), vec![0]);
    let reset = batch
        .history
        .previous_uploads(&candidate, &batch.bytes, false);
    assert_eq!(reset, vec![(0, batch.bytes[..STRIDE].to_vec())]);
    candidate.publish();
    assert!(!batch.history.has_pending());
}

#[test]
fn physical_acknowledgement_keeps_history_pending_without_current_edits() {
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
    let mut batch = batch();
    let a = batch.get(1).0;
    let b = batch.get(0).0;
    batch.set_object(1, b);
    let owner = Publications::default();
    assert_eq!(batch.pending_ranges_for(Some(&owner)), vec![0..2]);
    stage_current(&mut batch, &owner);
    batch.set_object(0, a);
    batch.prune_published(&owner);
    assert_eq!(batch.pending_ranges_for(Some(&owner)), vec![0..1]);
    stage_current(&mut batch, &owner);
    batch.prune_published(&owner);
    assert!(batch.pending_ranges_for(Some(&owner)).is_empty());
    batch.set_object(0, b);
    assert_eq!(batch.pending_ranges_for(Some(&owner)), vec![0..1]);
    assert_eq!(batch.pending_ranges(), vec![0..2]);
}

#[test]
fn repeated_flush_cpu_restores_skin_without_transform_edits() {
    let mut batch = batch();
    batch.skin = Some(SkinStream::default());
    for slot in 0..2 {
        batch.set_skin(slot, [0, 0, 2, 0]);
    }
    batch.commit(&batch.pending_ranges());
    let current = batch.bytes.clone();
    let owner = Publications::default();
    batch.set_skin(0, [4, 4, 2, 0]);
    stage_current(&mut batch, &owner);
    batch.set_skin(0, [0, 0, 2, 0]);
    batch.prune_published(&owner);
    assert_eq!(batch.pending_ranges_for(Some(&owner)), vec![0..1]);
    stage_current(&mut batch, &owner);
    batch.prune_published(&owner);
    assert!(batch.pending_ranges_for(Some(&owner)).is_empty());
    batch.set_skin(1, [8, 8, 2, 0]);
    assert_eq!(batch.pending_ranges_for(Some(&owner)), vec![1..2]);
    assert_eq!(batch.bytes, current);
}

#[test]
fn repeated_flush_cpu_keeps_unsubmitted_edits_pending_outside_the_owner() {
    let mut batch = batch();
    let owner = Publications::default();
    stage_current(&mut batch, &owner);
    batch.prune_published(&owner);
    assert!(batch.pending_ranges_for(Some(&owner)).is_empty());
    assert_eq!(batch.pending_ranges(), vec![0..1]);
    let other = Publications::default();
    batch.prune_published(&other);
    assert_eq!(batch.pending_ranges_for(Some(&other)), vec![0..1]);
    stage_current(&mut batch, &other);
    assert!(batch.pending_ranges_for(Some(&other)).is_empty());
    assert_eq!(batch.pending_ranges_for(Some(&owner)), vec![0..1]);
    assert_eq!(batch.pending_ranges(), vec![0..1]);
}
