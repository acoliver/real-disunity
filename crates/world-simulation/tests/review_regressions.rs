use glam::{Quat, Vec3};
use worldrunner_world_simulation::{
    ComponentDesc, DestroyWhen, FieldValue, LocalTransform, ObjectDesc, ObjectHandle, ParentSpace,
    ScriptMessage, World, WorldError, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn script(
    world: &mut World,
    object: ObjectHandle,
) -> worldrunner_world_simulation::ComponentHandle {
    world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Actor".into(),
            },
        )
        .unwrap()
}

#[test]
fn b1_remove_component_does_not_retire_unrelated_deferred_object() {
    let mut world = world();
    let queued = world.create_object(ObjectDesc::default()).unwrap();
    let other = world.create_object(ObjectDesc::default()).unwrap();
    let renderer = world
        .add_component(other, ComponentDesc::Renderer { material_slot: 0 })
        .unwrap();
    world.destroy(queued, DestroyWhen::EndOfFrame).unwrap();
    world.remove_component(renderer).unwrap();
    assert_eq!(world.storage_counts(), (2, 2));
    assert_eq!(world.destroy(queued, DestroyWhen::EndOfFrame), Ok(()));
    world.finish_phase();
    assert_eq!(world.storage_counts(), (1, 1));
}

#[test]
fn b1_destroy_object_does_not_retire_unrelated_deferred_object() {
    let mut world = world();
    let queued = world.create_object(ObjectDesc::default()).unwrap();
    let other = world.create_object(ObjectDesc::default()).unwrap();
    world.destroy(queued, DestroyWhen::EndOfFrame).unwrap();
    world.destroy_object(other).unwrap();
    assert_eq!(world.storage_counts(), (1, 1));
    assert_eq!(world.destroy(queued, DestroyWhen::EndOfFrame), Ok(()));
    world.finish_phase();
    assert_eq!(world.storage_counts(), (0, 0));
}

#[test]
fn b2_keep_world_preserves_collapsed_orientation() {
    for parent_rotation in [Quat::IDENTITY, Quat::from_rotation_z(0.4)] {
        for scale in [
            Vec3::ZERO,
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 3.0),
        ] {
            let mut world = world();
            let parent = world.create_object(ObjectDesc::default()).unwrap();
            let child = world.create_object(ObjectDesc::default()).unwrap();
            world
                .set_local_transform(
                    parent,
                    LocalTransform {
                        translation: Vec3::ONE,
                        rotation: parent_rotation,
                        ..Default::default()
                    },
                )
                .unwrap();
            let rotation = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.3);
            world
                .set_local_transform(
                    child,
                    LocalTransform {
                        rotation,
                        scale,
                        ..Default::default()
                    },
                )
                .unwrap();
            let matrix = world.world_matrix(child).unwrap();
            world
                .set_parent(child, Some(parent), ParentSpace::KeepWorld)
                .unwrap();
            let local = world.local_transform(child).unwrap();
            assert!(
                local
                    .rotation
                    .dot(parent_rotation.inverse() * rotation)
                    .abs()
                    > 1.0 - 1e-5
            );
            assert!(world.world_matrix(child).unwrap().abs_diff_eq(matrix, 1e-4));
            world
                .set_local_transform(
                    child,
                    LocalTransform {
                        scale: Vec3::ONE,
                        ..local
                    },
                )
                .unwrap();
            assert!(
                (parent_rotation * world.local_transform(child).unwrap().rotation)
                    .dot(rotation)
                    .abs()
                    > 1.0 - 1e-5
            );
        }
    }
}

#[test]
fn b5_component_destruction_coalesces_exact_queued_identities() {
    for owner_first in [false, true] {
        let mut world = world();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let script = script(&mut world, object);
        let renderer = world
            .add_component(object, ComponentDesc::Renderer { material_slot: 0 })
            .unwrap();
        world.set_active(object, true).unwrap();
        if owner_first {
            world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
        }
        for handle in [script, renderer] {
            world.destroy_component(handle).unwrap();
            world.destroy_component(handle).unwrap();
        }
        if !owner_first {
            world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
        }
        assert_eq!(
            world
                .take_cleanup()
                .iter()
                .map(|v| v.message)
                .collect::<Vec<_>>(),
            vec![ScriptMessage::OnDisable, ScriptMessage::OnDestroy]
        );
        assert!(world.take_cleanup().is_empty());
        world.finish_phase();
        for handle in [script, renderer] {
            assert_eq!(
                world.destroy_component(handle),
                Err(WorldError::StaleComponent(handle))
            );
        }
    }
}

#[test]
fn b6_script_fields_reject_foreign_references_atomically() {
    let mut world = world();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = script(&mut world, object);
    let mut foreign = World::new(WorldSettings::default()).unwrap();
    let other = foreign.create_object(ObjectDesc::default()).unwrap();
    let component = foreign.object(other).unwrap().transform;
    let original = vec![("same_world".into(), FieldValue::Object(object))];
    world.set_script_fields(script, original.clone()).unwrap();
    for reference in [FieldValue::Object(other), FieldValue::Component(component)] {
        for value in [
            reference.clone(),
            FieldValue::Record(vec![(
                "nested".into(),
                FieldValue::Array(vec![reference.clone()]),
            )]),
        ] {
            assert_eq!(
                world.set_script_fields(
                    script,
                    vec![
                        ("valid".into(), FieldValue::Null),
                        ("foreign".into(), value)
                    ]
                ),
                Err(WorldError::WrongWorld)
            );
            assert_eq!(world.script_fields(script).unwrap(), original);
        }
    }
}
