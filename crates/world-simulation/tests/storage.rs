use glam::{Quat, Vec3};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, LimitKind, LocalTransform, ObjectDesc, World, WorldError,
    WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

#[test]
fn worlds_reject_each_others_handles() {
    let mut a = world();
    let mut b = world();
    let object = a.create_object(ObjectDesc::default()).unwrap();
    let other = b.create_object(ObjectDesc::default()).unwrap();
    assert_ne!(object, other);
    assert_eq!(b.object(object), Err(WorldError::WrongWorld));
    let component = a.object(object).unwrap().transform;
    assert_eq!(b.component(component), Err(WorldError::WrongWorld));
}

#[test]
fn slot_reuse_never_resurrects_objects_or_components() {
    let mut w = World::new(WorldSettings {
        max_objects: 1,
        max_components: 1,
    })
    .unwrap();
    let first = w.create_object(ObjectDesc::default()).unwrap();
    let transform = w.object(first).unwrap().transform;
    w.destroy_object(first).unwrap();
    let second = w.create_object(ObjectDesc::default()).unwrap();
    assert_ne!(first, second);
    assert_eq!(w.object(first), Err(WorldError::StaleObject(first)));
    assert_eq!(
        w.component(transform),
        Err(WorldError::StaleComponent(transform))
    );
    assert_eq!(w.snapshot().objects.len(), 1);
}

#[test]
fn typed_component_storage_has_one_transform_and_owned_components() {
    let mut w = world();
    let object = w.create_object(ObjectDesc::default()).unwrap();
    let component = w
        .add_component(object, ComponentDesc::Renderer { material_slot: 7 })
        .unwrap();
    assert_eq!(w.component(component).unwrap().object, object);
    assert_eq!(
        w.component(component).unwrap().desc,
        ComponentDesc::Renderer { material_slot: 7 }
    );
    assert_eq!(
        w.add_component(object, ComponentDesc::Transform(LocalTransform::default())),
        Err(WorldError::InvalidComponent(ComponentError::Duplicate))
    );
    w.remove_component(component).unwrap();
    assert_eq!(
        w.component(component),
        Err(WorldError::StaleComponent(component))
    );
    w.destroy_object(object).unwrap();
    assert!(w.snapshot().components.is_empty());
}

#[test]
fn invalid_mutations_do_not_change_the_snapshot() {
    let mut w = world();
    let object = w.create_object(ObjectDesc::default()).unwrap();
    let before = w.snapshot();
    let invalid = LocalTransform {
        translation: Vec3::splat(f32::NAN),
        ..Default::default()
    };
    assert!(w.set_local_transform(object, invalid).is_err());
    assert_eq!(w.snapshot(), before);
    let transform = w.object(object).unwrap().transform;
    assert_eq!(
        w.remove_component(transform),
        Err(WorldError::InvalidComponent(
            ComponentError::RequiredTransform
        ))
    );
    assert_eq!(w.snapshot(), before);
}

#[test]
fn object_allocation_checks_component_capacity_before_mutating() {
    let settings = WorldSettings {
        max_objects: 2,
        max_components: 1,
    };
    let mut w = World::new(settings).unwrap();
    w.create_object(ObjectDesc::default()).unwrap();
    let before = w.snapshot();
    assert_eq!(
        w.create_object(ObjectDesc::default()),
        Err(WorldError::CapacityExceeded(LimitKind::Components))
    );
    assert_eq!(w.snapshot(), before);
}

#[test]
fn transforms_use_quaternions_and_accept_zero_scale() {
    let mut w = world();
    let object = w.create_object(ObjectDesc::default()).unwrap();
    let value = LocalTransform {
        translation: Vec3::new(1.0, 2.0, 3.0),
        rotation: Quat::from_rotation_y(0.75),
        scale: Vec3::ZERO,
    };
    w.set_local_transform(object, value).unwrap();
    assert_eq!(w.local_transform(object).unwrap(), value);
    assert_eq!(
        w.world_matrix(object).unwrap().w_axis.truncate(),
        value.translation
    );
}

#[test]
fn retired_world_identity_is_not_reused() {
    let stale = {
        let mut w = world();
        w.create_object(ObjectDesc::default()).unwrap()
    };
    let mut next = world();
    next.create_object(ObjectDesc::default()).unwrap();
    assert_eq!(next.object(stale), Err(WorldError::WrongWorld));
}

#[test]
fn object_alive_names_only_live_objects_of_the_same_world() {
    let mut first = world();
    let second = world();
    let kept = first.create_object(ObjectDesc::default()).unwrap();
    let removed = first.create_object(ObjectDesc::default()).unwrap();
    first.destroy_object(removed).unwrap();
    assert!(first.object_alive(kept));
    assert!(!first.object_alive(removed));
    assert!(!second.object_alive(kept));
}
