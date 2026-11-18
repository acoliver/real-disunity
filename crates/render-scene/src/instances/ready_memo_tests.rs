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
        batch
            .history
            .prepare(&batch.bytes, &batch.dirty, owner)
            .publish();
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
fn no_transaction_and_another_transaction_are_checked_on_their_own() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let owner = Owner::new();
    let (instances, _) = settled(&owner);
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
