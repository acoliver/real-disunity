//! `Instances::ready_for` reuses a passing scan only while nothing the scan reads has changed.
use super::*;
use crate::instances::{InstanceHandle, Instances, test_owner::Owner};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneGeometry, SceneParts, SceneResources,
    SceneRotation,
};

fn object() -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}

fn moved(x: i64) -> SceneTransform {
    SceneTransform {
        position_millimeters: [x, 0, 0],
        ..*object().transform()
    }
}

/// Two uploaded placements whose last-colour history is published, so every batch is ready.
fn settled(owner: &Publications) -> (Instances, InstanceHandle) {
    fixture(owner, true)
}

/// Uploaded placements whose last-colour history is prepared but not yet published: every batch
/// is ready, and its history still has work pending.
fn prepared(owner: &Publications) -> (Instances, InstanceHandle) {
    fixture(owner, false)
}

fn fixture(owner: &Publications, publish: bool) -> (Instances, InstanceHandle) {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut instances = Instances::new(&scene);
    instances.mesh_count = 1;
    instances.pipelines.push(0);
    let handle = instances.add(object(), true).unwrap();
    instances.add(object(), true).unwrap();
    for batch in instances.batches.values_mut() {
        batch.commit(&batch.pending_ranges());
        let candidate = batch.history.prepare(&batch.bytes, &batch.dirty, owner);
        if publish {
            candidate.publish();
        }
    }
    (instances, handle)
}

#[test]
fn an_unchanged_batch_set_is_scanned_once_for_a_transaction() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (instances, _) = settled(&owner);
    assert!(instances.ready_for(Some(&owner)));
    assert!(instances.ready_for(Some(&owner)));
    assert!(instances.ready_for(Some(&owner)));
    assert_eq!(instances.ready_scans(), 1);
}

#[test]
fn an_idle_scene_is_scanned_once_whoever_asks() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (instances, _) = settled(&owner);
    assert!(instances.batches.values().all(InstanceBatch::is_idle));
    let other = Owner::new();
    assert!(instances.ready_for(Some(&owner)));
    assert!(instances.ready_for(None));
    assert!(instances.ready_for(Some(&other)));
    assert!(instances.ready_for(Some(&owner)));
    assert_eq!(instances.ready_scans(), 1);
}

#[test]
fn an_edit_after_an_idle_scan_is_refused_to_every_asker() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let other = Owner::new();
    let (mut instances, handle) = settled(&owner);
    assert!(instances.ready_for(Some(&other)));
    instances.set_transform(handle, moved(250)).unwrap();
    assert!(!instances.ready_for(Some(&owner)));
    assert!(!instances.ready_for(Some(&other)));
    assert!(!instances.ready_for(None));
}

#[test]
fn work_pending_in_the_history_keeps_the_scene_from_being_idle() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (instances, _) = prepared(&owner);
    assert!(instances.batches.values().all(|batch| !batch.is_idle()));
    assert!(instances.ready_for(Some(&owner)));
    assert!(instances.ready_for(None));
    assert!(instances.ready_for(None));
    assert_eq!(
        instances.ready_scans(),
        2,
        "each asker is checked on its own"
    );
}

#[test]
fn no_transaction_and_another_transaction_are_checked_on_their_own() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (instances, _) = prepared(&owner);
    assert!(instances.ready_for(Some(&owner)));
    assert!(instances.ready_for(None));
    assert!(instances.ready_for(None));
    assert_eq!(instances.ready_scans(), 2);
    let other = Owner::new();
    assert!(instances.ready_for(Some(&other)));
    assert_eq!(instances.ready_scans(), 3);
}

#[test]
fn an_edit_after_a_passing_check_is_refused_in_the_same_transaction() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (mut instances, handle) = settled(&owner);
    assert!(instances.ready_for(Some(&owner)));
    instances.set_transform(handle, moved(250)).unwrap();
    assert!(!instances.ready_for(Some(&owner)));
    assert!(!instances.ready_for(Some(&owner)));
}

#[test]
fn an_edit_after_a_passing_check_is_refused_without_a_transaction() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (mut instances, handle) = settled(&owner);
    assert!(instances.ready_for(None));
    instances.set_transform(handle, moved(250)).unwrap();
    assert!(!instances.ready_for(None));
}

#[test]
fn a_velocity_reset_after_a_passing_check_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (mut instances, _) = settled(&owner);
    assert!(instances.ready_for(Some(&owner)));
    for batch in instances.batches.values_mut() {
        batch.reset_velocity_history();
    }
    assert!(!instances.ready_for(Some(&owner)));
}

#[test]
fn a_new_placement_after_a_passing_check_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (mut instances, _) = settled(&owner);
    assert!(instances.ready_for(Some(&owner)));
    instances.add(object(), true).unwrap();
    assert!(!instances.ready_for(Some(&owner)));
}

#[test]
fn a_failed_scan_is_not_remembered() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (mut instances, handle) = settled(&owner);
    instances.set_transform(handle, moved(250)).unwrap();
    assert!(!instances.ready_for(Some(&owner)));
    assert!(!instances.ready_for(Some(&owner)));
    assert_eq!(instances.ready_scans(), 2);
}
