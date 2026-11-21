//! The counters batches expose in sums (placement edits, staged content, history changes, skin
//! streams) are read many times a frame, so the scene keeps them as it changes instead of adding
//! them up over every batch on each ask.
use super::*;
use crate::instances::{InstanceHandle, Instances, test_owner::Owner};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneGeometry, SceneParts, SceneResources,
    SceneRotation,
};

fn object(material: u32) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(material),
    )
}

fn moved(x: i64) -> SceneTransform {
    SceneTransform {
        position_millimeters: [x, 0, 0],
        ..*object(0).transform()
    }
}

/// Placements spread over three batches (one pipeline index and material each).
fn scene_of_three_batches() -> (Instances, Vec<InstanceHandle>) {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut instances = Instances::new(&scene);
    instances.mesh_count = 1;
    instances.pipelines.extend([0, 1, 2]);
    let handles = (0..9)
        .map(|index| instances.add(object(index % 3), true).unwrap())
        .collect();
    (instances, handles)
}

/// What each counter adds up to over the batches, which the scene's own figures must equal.
fn reference(instances: &Instances) -> (u64, u64, u64, bool) {
    (
        instances.batches.values().map(InstanceBatch::edits).sum(),
        instances.batches.values().map(InstanceBatch::content).sum(),
        instances
            .batches
            .values()
            .map(InstanceBatch::history_version)
            .sum(),
        instances.batches.values().any(|batch| batch.skin.is_some()),
    )
}

fn figures(instances: &Instances) -> (u64, u64, u64, bool) {
    (
        instances.edits(),
        instances.content_generation(),
        instances.history_versions(),
        instances.has_skinned_batches(),
    )
}

#[test]
fn asking_for_the_counters_scans_no_batch_until_something_changes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (mut instances, handles) = scene_of_three_batches();
    for batch in instances.batches.values_mut() {
        batch.commit(&batch.pending_ranges());
        batch
            .history
            .prepare(&batch.bytes, &batch.dirty, &owner)
            .publish();
    }
    assert!(instances.ready_for(Some(&owner)));
    let scans = instances.ready_scans();
    for _ in 0..5 {
        assert!(instances.ready_for(Some(&owner)));
        assert!(instances.ready_for(None));
        instances.layout_stamp(1);
        figures(&instances);
    }
    instances.set_transform(handles[4], moved(7)).unwrap();
    assert!(!instances.ready_for(Some(&owner)));
    figures(&instances);
    assert_eq!(
        instances.ready_scans(),
        scans + 1,
        "only the ask after the edit scanned the batches"
    );
}

#[test]
fn the_figures_equal_the_sums_through_every_kind_of_edit() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (mut instances, handles) = scene_of_three_batches();
    assert_eq!(figures(&instances), reference(&instances));
    for (index, &handle) in handles.iter().enumerate() {
        match index % 4 {
            0 => instances
                .set_transform(handle, moved(i64::try_from(index).unwrap() * 100))
                .unwrap(),
            1 => instances.set_visible(handle, false).unwrap(),
            2 => {
                let (placed, _) = instances.get(handle).unwrap();
                instances
                    .set_object(handle, placed.with_transform(moved(5)))
                    .unwrap();
            }
            _ => instances.remove(handle).unwrap(),
        }
        assert_eq!(figures(&instances), reference(&instances), "edit {index}");
    }
    instances.add(object(1), true).unwrap();
    instances.forget_mesh_bounds(0);
    for batch in instances.batches.values_mut() {
        batch.reset_velocity_history();
    }
    assert_eq!(figures(&instances), reference(&instances));
    assert!(instances.edits() > 0);
}

#[test]
fn only_a_scene_with_a_skinned_mesh_has_skinned_batches() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (rigid, _) = scene_of_three_batches();
    assert!(!rigid.has_skinned_batches());
    let mut skinned = Instances::new(&crate::draw::tests::skinning::posed(0));
    assert!(skinned.has_skinned_batches());
    let handles: Vec<_> = skinned.handles().collect();
    for handle in handles {
        skinned.remove(handle).unwrap();
    }
    assert!(
        skinned.has_skinned_batches(),
        "a batch stays after its placements are removed"
    );
}
