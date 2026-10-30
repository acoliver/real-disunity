use glam::{Quat, Vec2, Vec3};
use worldrunner_world_simulation::physics2d::{Body2d, BodyType};
use worldrunner_world_simulation::{
    ComponentChange, ComponentChangeEntry, ComponentDesc, ComponentKind, DestroyWhen,
    LocalTransform, ObjectDesc, ObjectEdit, ObjectHandle, ParentSpace, World, WorldChanges,
    WorldError, WorldSettings,
};

fn tracked() -> World {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.track_changes();
    world
}

fn quiet_object(world: &mut World) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world.drain_changes();
    object
}

fn renderer(slot: u32) -> ComponentDesc {
    ComponentDesc::Renderer {
        material_slot: slot,
    }
}

fn entry(
    object: ObjectHandle,
    kind: ComponentKind,
    change: ComponentChange,
) -> ComponentChangeEntry {
    ComponentChangeEntry {
        object,
        kind,
        change,
    }
}

#[test]
fn tracking_is_off_by_default_and_records_nothing() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert!(!world.is_tracking_changes());
    let a = world.create_object(ObjectDesc::default()).unwrap();
    let b = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_local_transform(a, LocalTransform::default())
        .unwrap();
    world
        .set_parent(b, Some(a), ParentSpace::KeepLocal)
        .unwrap();
    world.add_component(a, renderer(1)).unwrap();
    world.set_layer(a, 3).unwrap();
    world.destroy_object(b).unwrap();
    assert!(world.drain_changes().is_empty());
    world.track_changes();
    assert!(world.drain_changes().is_empty());
}

#[test]
fn drain_clears_the_interval() {
    let mut world = tracked();
    let a = world.create_object(ObjectDesc::default()).unwrap();
    assert_eq!(world.drain_changes().created, vec![a]);
    assert!(world.drain_changes().is_empty());
}

#[test]
fn create_records_the_object() {
    let mut world = tracked();
    let a = world.create_object(ObjectDesc::default()).unwrap();
    let b = world.create_object(ObjectDesc::default()).unwrap();
    assert_eq!(
        world.drain_changes(),
        WorldChanges {
            created: vec![a, b],
            ..WorldChanges::default()
        }
    );
}

#[test]
fn repeated_transform_changes_yield_one_entry() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    for x in [1.0_f32, 2.0, 3.0] {
        let value = LocalTransform {
            translation: Vec3::new(x, 0.0, 0.0),
            ..Default::default()
        };
        world.set_local_transform(a, value).unwrap();
    }
    world.set_world_position(a, Vec3::ONE).unwrap();
    world
        .set_world_rotation(a, Quat::from_rotation_z(0.5))
        .unwrap();
    assert_eq!(
        world.drain_changes(),
        WorldChanges {
            transforms: vec![a],
            ..WorldChanges::default()
        }
    );
}

#[test]
fn create_then_destroy_in_one_interval_yields_nothing() {
    let mut world = tracked();
    let a = world.create_object(ObjectDesc::default()).unwrap();
    world.add_component(a, renderer(1)).unwrap();
    world.set_layer(a, 2).unwrap();
    world.destroy_object(a).unwrap();
    assert!(world.drain_changes().is_empty());
}

#[test]
fn destroy_drops_other_entries_for_that_object() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    let b = quiet_object(&mut world);
    world.add_component(a, renderer(1)).unwrap();
    world
        .set_local_transform(a, LocalTransform::default())
        .unwrap();
    world
        .set_parent(a, Some(b), ParentSpace::KeepLocal)
        .unwrap();
    world.set_active(a, false).unwrap();
    world.destroy_object(a).unwrap();
    assert_eq!(
        world.drain_changes(),
        WorldChanges {
            destroyed: vec![a],
            ..WorldChanges::default()
        }
    );
}

#[test]
fn entries_are_ordered_by_object() {
    let mut world = tracked();
    let objects: Vec<_> = (0..4).map(|_| quiet_object(&mut world)).collect();
    for &object in objects.iter().rev() {
        world.set_layer(object, 1).unwrap();
        world.add_component(object, renderer(1)).unwrap();
    }
    let changes = world.drain_changes();
    assert_eq!(changes.properties, objects);
    let owners: Vec<_> = changes.components.iter().map(|c| c.object).collect();
    assert_eq!(owners, objects);
}

#[test]
fn component_changes_coalesce_per_kind() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    let kept = world.add_component(a, renderer(1)).unwrap();
    world.drain_changes();

    world.set_component(kept, renderer(2)).unwrap();
    world.set_component(kept, renderer(3)).unwrap();
    assert_eq!(
        world.drain_changes().components,
        vec![entry(a, ComponentKind::Renderer, ComponentChange::Changed)]
    );

    world.remove_component(kept).unwrap();
    let again = world.add_component(a, renderer(4)).unwrap();
    assert_eq!(
        world.drain_changes().components,
        vec![entry(a, ComponentKind::Renderer, ComponentChange::Changed)]
    );

    world.remove_component(again).unwrap();
    assert_eq!(
        world.drain_changes().components,
        vec![entry(a, ComponentKind::Renderer, ComponentChange::Removed)]
    );

    let fresh = world.add_component(a, renderer(5)).unwrap();
    world.set_component(fresh, renderer(6)).unwrap();
    assert_eq!(
        world.drain_changes().components,
        vec![entry(a, ComponentKind::Renderer, ComponentChange::Added)]
    );

    world.remove_component(fresh).unwrap();
    assert_eq!(
        world.drain_changes().components,
        vec![entry(a, ComponentKind::Renderer, ComponentChange::Removed)]
    );

    let transient = world.add_component(a, renderer(7)).unwrap();
    world.remove_component(transient).unwrap();
    assert!(world.drain_changes().is_empty());
}

#[test]
fn reparenting_records_the_parent_and_the_rewritten_transform() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    let b = quiet_object(&mut world);
    world
        .set_parent(a, Some(b), ParentSpace::KeepLocal)
        .unwrap();
    let changes = world.drain_changes();
    assert_eq!(changes.parents, vec![a]);
    assert_eq!(changes.transforms, vec![a]);
    assert!(changes.created.is_empty() && changes.destroyed.is_empty());
}

#[test]
fn object_properties_record_a_property_change() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    world.set_layer(a, 4).unwrap();
    world.set_object_text(a, true, "Tag".into()).unwrap();
    world.set_object_text(a, false, "Name".into()).unwrap();
    world.set_active(a, false).unwrap();
    assert_eq!(
        world.drain_changes(),
        WorldChanges {
            properties: vec![a],
            ..WorldChanges::default()
        }
    );
}

#[test]
fn deferred_destroy_records_when_storage_retires() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    world.destroy(a, DestroyWhen::EndOfFrame).unwrap();
    assert!(world.drain_changes().is_empty());
    world.finish_phase();
    assert_eq!(world.drain_changes().destroyed, vec![a]);
}

#[test]
fn destroying_a_subtree_reports_every_retired_object() {
    let mut world = tracked();
    let parent = quiet_object(&mut world);
    let child = quiet_object(&mut world);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world.drain_changes();
    world.destroy_object(parent).unwrap();
    assert_eq!(world.drain_changes().destroyed, vec![parent, child]);
}

#[test]
fn apply_edits_records_transforms_and_renderers() {
    let mut world = tracked();
    let a = quiet_object(&mut world);
    let b = quiet_object(&mut world);
    world
        .apply_edits(&[
            ObjectEdit::Translation(a, Vec3::X),
            ObjectEdit::Rotation(a, Quat::from_rotation_y(0.25)),
            ObjectEdit::MaterialSlot(b, 3),
        ])
        .unwrap();
    assert_eq!(
        world.drain_changes(),
        WorldChanges {
            transforms: vec![a],
            components: vec![entry(b, ComponentKind::Renderer, ComponentChange::Added)],
            ..WorldChanges::default()
        }
    );
}

#[test]
fn rejected_apply_edits_leaves_the_journal_untouched() {
    let mut world = tracked();
    let live = quiet_object(&mut world);
    let stale = quiet_object(&mut world);
    world.destroy_object(stale).unwrap();
    world.set_layer(live, 1).unwrap();
    let before = world.snapshot();
    let result = world.apply_edits(&[
        ObjectEdit::Translation(live, Vec3::ONE),
        ObjectEdit::MaterialSlot(stale, 4),
    ]);
    assert_eq!(result, Err(WorldError::StaleObject(stale)));
    assert_eq!(world.snapshot(), before);
    assert_eq!(
        world.drain_changes(),
        WorldChanges {
            destroyed: vec![stale],
            properties: vec![live],
            ..WorldChanges::default()
        }
    );
}

fn mover(world: &mut World) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Kinematic,
                velocity: Vec2::new(2.0, 0.0),
                gravity_scale: 0.0,
                ..Default::default()
            }),
        )
        .unwrap();
    object
}

#[test]
fn step_with_edits_publishes_edits_and_motion_and_records_them() {
    let mut world = tracked();
    let body = mover(&mut world);
    let other = quiet_object(&mut world);
    world.drain_changes();
    world
        .step_with_edits(&[ObjectEdit::MaterialSlot(other, 2)], 0.5)
        .unwrap();
    assert!((world.local_transform(body).unwrap().translation.x - 1.0).abs() < 1e-6);
    assert!(world.renderer(other).unwrap().is_some());
    let changes = world.drain_changes();
    assert_eq!(changes.transforms, vec![body]);
    assert_eq!(
        changes.components,
        vec![
            entry(body, ComponentKind::Body2d, ComponentChange::Changed),
            entry(other, ComponentKind::Renderer, ComponentChange::Added),
        ]
    );
}

#[test]
fn failed_batch_leaves_world_and_journal_unchanged() {
    let mut world = tracked();
    let body = mover(&mut world);
    let target = quiet_object(&mut world);
    let reused = world.add_component(target, renderer(1)).unwrap();
    world.remove_component(reused).unwrap();
    world.set_layer(target, 5).unwrap();
    let snapshot = world.snapshot();
    let counts = world.storage_counts();
    let edits = [
        ObjectEdit::Translation(body, Vec3::new(9.0, 0.0, 0.0)),
        ObjectEdit::Rotation(target, Quat::from_rotation_z(1.0)),
        ObjectEdit::MaterialSlot(target, 8),
    ];
    assert!(matches!(
        world.step_with_edits(&edits, -1.0),
        Err(WorldError::Physics(_))
    ));
    assert_eq!(world.snapshot(), snapshot);
    assert_eq!(world.storage_counts(), counts);
    assert_eq!(world.renderer(target).unwrap(), None);
    let next = world.add_component(target, renderer(2)).unwrap();
    assert!(format!("{next:?}").contains("generation: 2"), "{next:?}");
    world.remove_component(next).unwrap();
    let changes = world.drain_changes();
    assert_eq!(changes.properties, vec![target]);
    assert!(changes.transforms.is_empty());
    assert!(changes.components.is_empty());
    assert!(changes.created.is_empty() && changes.destroyed.is_empty());
}

#[test]
fn failed_batch_restores_physics_state() {
    let mut failing = tracked();
    let body = mover(&mut failing);
    let result = failing.step_with_edits(&[ObjectEdit::Translation(body, Vec3::ONE)], f32::NAN);
    assert!(result.is_err());
    failing.step_with_edits(&[], 0.5).unwrap();
    assert!((failing.local_transform(body).unwrap().translation.x - 1.0).abs() < 1e-6);
}
