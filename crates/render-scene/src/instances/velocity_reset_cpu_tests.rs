use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation};

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
    batch.fresh.borrow_mut().clear();
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
    let before = previous_uploads(&batch, &batch.pending_ranges(), false);
    assert_ne!(before.uploads[0].1, batch.bytes[..STRIDE]);
    batch.reset_velocity_history();
    for _ in 0..2 {
        let reset = previous_uploads(&batch, &batch.pending_ranges(), false);
        assert_eq!(reset.uploads, vec![(0, batch.bytes.clone())]);
        assert!(reset.slots.is_empty());
    }
}

#[test]
fn velocity_reset_cpu_ignores_removed_slots_and_keeps_current_uploads_sparse() {
    let mut batch = batch();
    batch.remove(1);
    let writes = batch.pending_ranges();
    batch.reset_velocity_history();
    assert_eq!(batch.pending_ranges(), writes);
    assert_eq!(batch.live_stale().collect::<Vec<_>>(), vec![0]);
    let reset = previous_uploads(&batch, &writes, false);
    assert_eq!(reset.uploads, vec![(0, batch.bytes[..STRIDE].to_vec())]);
}
