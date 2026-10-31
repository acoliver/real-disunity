//! Object and component creation must not depend on how many objects the world already holds.
//!
//! The scaling test asserts a wall-clock bound. Creation used to scan every slot, which made
//! 300,000 objects take hours; the bound is set two orders of magnitude above the measured
//! debug-build time so that scheduler noise cannot fail it while a return to a linear scan
//! per create still does.

use std::time::{Duration, Instant};

use glam::Vec3;
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, ObjectEdit, ObjectHandle, World, WorldSettings,
};

const OBJECTS: usize = 300_000;
const BOUND: Duration = Duration::from_mins(1);

fn at(x: f32) -> ObjectDesc {
    ObjectDesc {
        local_transform: LocalTransform {
            translation: Vec3::new(x, 0.0, 0.0),
            ..LocalTransform::default()
        },
        ..ObjectDesc::default()
    }
}

#[test]
fn three_hundred_thousand_objects_create_destroy_and_recreate_in_bounded_time() {
    let started = Instant::now();
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut objects: Vec<ObjectHandle> = (0..OBJECTS)
        .map(|index| {
            world
                .create_object(at(f32::from(u16::try_from(index % 1000).unwrap())))
                .unwrap()
        })
        .collect();
    for index in (0..OBJECTS).step_by(10) {
        world.destroy_object(objects[index]).unwrap();
    }
    assert_eq!(
        world.storage_counts(),
        (OBJECTS - OBJECTS / 10, OBJECTS - OBJECTS / 10)
    );
    for index in (0..OBJECTS).step_by(10) {
        let old = objects[index];
        let fresh = world.create_object(at(-1.0)).unwrap();
        assert_ne!(fresh, old, "a reused slot carries a new generation");
        objects[index] = fresh;
    }
    assert_eq!(world.storage_counts(), (OBJECTS, OBJECTS));
    for &object in objects.iter().step_by(7) {
        world
            .set_local_transform(object, LocalTransform::default())
            .unwrap();
    }
    assert!(
        started.elapsed() < BOUND,
        "300k create, 30k destroy and recreate took {:?}",
        started.elapsed()
    );
}

#[test]
fn destroying_a_child_leaves_its_siblings_and_parent_consistent() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    let kids: Vec<_> = (0..4)
        .map(|_| {
            let kid = world.create_object(ObjectDesc::default()).unwrap();
            world
                .set_parent(
                    kid,
                    Some(parent),
                    worldrunner_world_simulation::ParentSpace::KeepLocal,
                )
                .unwrap();
            kid
        })
        .collect();
    world.destroy_object(kids[1]).unwrap();
    assert_eq!(
        world.children(parent).unwrap(),
        vec![kids[0], kids[2], kids[3]]
    );
    let replacement = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(
            replacement,
            Some(parent),
            worldrunner_world_simulation::ParentSpace::KeepLocal,
        )
        .unwrap();
    assert_eq!(
        world.children(parent).unwrap(),
        vec![kids[0], replacement, kids[2], kids[3]]
    );
}

/// Handles embed the world identity, so two worlds are compared by slot and generation only.
fn position(handle: impl std::fmt::Debug) -> String {
    let text = format!("{handle:?}");
    text[text.find("slot").unwrap()..].to_owned()
}

fn churned_world() -> (World, Vec<ObjectHandle>) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let objects: Vec<_> = (0..8)
        .map(|_| world.create_object(ObjectDesc::default()).unwrap())
        .collect();
    for &object in &[objects[1], objects[4], objects[5]] {
        world.destroy_object(object).unwrap();
    }
    let _refill = world.create_object(ObjectDesc::default()).unwrap();
    (world, objects)
}

#[test]
fn failed_edit_batch_leaves_the_next_handles_unchanged() {
    let (mut failed, objects) = churned_world();
    let (mut control, control_objects) = churned_world();
    let edits = |objects: &[ObjectHandle]| {
        [
            ObjectEdit::MaterialSlot(objects[0], 1),
            ObjectEdit::MaterialSlot(objects[2], 2),
            ObjectEdit::Translation(objects[3], Vec3::ONE),
        ]
    };
    assert!(failed.step_with_edits(&edits(&objects), -1.0).is_err());

    for _ in 0..4 {
        let a = failed.create_object(ObjectDesc::default()).unwrap();
        let b = control.create_object(ObjectDesc::default()).unwrap();
        assert_eq!(position(a), position(b));
    }
    let a = failed
        .add_component(objects[6], ComponentDesc::Renderer { material_slot: 1 })
        .unwrap();
    let b = control
        .add_component(
            control_objects[6],
            ComponentDesc::Renderer { material_slot: 1 },
        )
        .unwrap();
    assert_eq!(position(a), position(b));
}
