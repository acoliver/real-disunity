use worldrunner_world_simulation::{
    ComponentChange, ComponentChangeEntry, ComponentDesc, ComponentError, ComponentKind,
    MaterialId, MeshId, MeshInstance, ObjectDesc, ObjectHandle, ParentSpace, World, WorldError,
    WorldSettings,
};

fn tracked() -> World {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.track_changes();
    world
}

fn instance(mesh: u64) -> MeshInstance {
    MeshInstance::new(MeshId(mesh), MaterialId(mesh + 100))
}

fn entry(object: ObjectHandle, change: ComponentChange) -> ComponentChangeEntry {
    ComponentChangeEntry {
        object,
        kind: ComponentKind::MeshInstance,
        change,
    }
}

#[test]
fn adding_changing_and_removing_a_mesh_instance_is_journaled() {
    let mut world = tracked();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world.drain_changes();

    let handle = world
        .add_component(object, ComponentDesc::MeshInstance(instance(1)))
        .unwrap();
    assert_eq!(
        world.drain_changes().components,
        [entry(object, ComponentChange::Added)]
    );
    assert_eq!(world.mesh_instance(object).unwrap(), Some(&instance(1)));

    world
        .set_component(handle, ComponentDesc::MeshInstance(instance(2)))
        .unwrap();
    assert_eq!(
        world.drain_changes().components,
        [entry(object, ComponentChange::Changed)]
    );
    assert_eq!(world.mesh_instance(object).unwrap(), Some(&instance(2)));

    world.remove_component(handle).unwrap();
    assert_eq!(
        world.drain_changes().components,
        [entry(object, ComponentChange::Removed)]
    );
    assert_eq!(world.mesh_instance(object).unwrap(), None);
}

#[test]
fn objects_with_mesh_instances_are_listed_in_handle_order() {
    let mut world = tracked();
    let a = world.create_object(ObjectDesc::default()).unwrap();
    let plain = world.create_object(ObjectDesc::default()).unwrap();
    let c = world.create_object(ObjectDesc::default()).unwrap();
    for object in [c, a] {
        world
            .add_component(object, ComponentDesc::MeshInstance(instance(1)))
            .unwrap();
    }
    assert_eq!(world.mesh_instance(plain).unwrap(), None);
    assert_eq!(world.mesh_instance_objects().collect::<Vec<_>>(), [a, c]);
}

#[test]
fn a_second_mesh_instance_on_one_object_is_rejected() {
    let mut world = tracked();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::MeshInstance(instance(1)))
        .unwrap();
    assert_eq!(
        world.add_component(object, ComponentDesc::MeshInstance(instance(2))),
        Err(WorldError::InvalidComponent(ComponentError::Duplicate))
    );
}

#[test]
fn invalid_mesh_instances_are_rejected() {
    let mut world = tracked();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let no_materials = MeshInstance {
        materials: Vec::new(),
        ..instance(1)
    };
    let too_bright = MeshInstance {
        tint: [1.0, 4.5, 1.0],
        ..instance(1)
    };
    let not_finite = MeshInstance {
        tint: [f32::NAN, 1.0, 1.0],
        ..instance(1)
    };
    let too_many_lods = MeshInstance {
        lods: vec![MeshId(2); 9],
        ..instance(1)
    };
    for bad in [no_materials, too_bright, not_finite, too_many_lods] {
        assert_eq!(
            world.add_component(object, ComponentDesc::MeshInstance(bad)),
            Err(WorldError::InvalidComponent(
                ComponentError::InvalidMeshInstance
            ))
        );
    }
}

#[test]
fn descendants_lists_every_level_below_the_roots() {
    let mut world = tracked();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    let grandchild = world.create_object(ObjectDesc::default()).unwrap();
    let other = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(grandchild, Some(child), ParentSpace::KeepLocal)
        .unwrap();
    assert_eq!(world.descendants(&[root]), [child, grandchild]);
    assert_eq!(world.descendants(&[other]), []);
    assert_eq!(world.descendants(&[]), []);
}
