//! `published_components` lends the components `snapshot` copies, in the same order.
use worldrunner_world_simulation::{
    ComponentDesc, ComponentSnapshot, ObjectDesc, ObjectHandle, World, WorldSettings,
};

fn script(world: &mut World, object: ObjectHandle, name: &str) {
    world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: name.into(),
            },
        )
        .unwrap();
}

fn lent(world: &World) -> Vec<ComponentSnapshot> {
    world.published_components().cloned().collect()
}

fn handles(world: &World) -> Vec<ObjectHandle> {
    world.snapshot().objects.iter().map(|o| o.handle).collect()
}

#[test]
fn lends_what_a_snapshot_holds_in_the_same_order() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert!(lent(&world).is_empty());
    for name in ["A", "B", "C"] {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        script(&mut world, object, name);
    }
    let lent = lent(&world);
    assert_eq!(lent.len(), 6, "a transform and a script on each object");
    assert_eq!(lent, world.snapshot().components);
    assert_eq!(world.published_objects().count(), 3);
    assert!(world.published_objects().eq(handles(&world)));
}

#[test]
fn leaves_out_the_components_of_unpublished_objects() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let first = world.create_object(ObjectDesc::default()).unwrap();
    script(&mut world, first, "First");
    let construction = world.begin_construction().unwrap();
    let pending = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    script(&mut world, pending, "Pending");
    let last = world.create_object(ObjectDesc::default()).unwrap();
    script(&mut world, last, "Last");
    let before = lent(&world);
    assert_eq!(before.len(), 4);
    assert!(before.iter().all(|c| c.object != pending));
    assert_eq!(before, world.snapshot().components);
    assert!(world.published_objects().eq(handles(&world)));
    assert!(world.published_objects().all(|o| o != pending));
    world.publish_construction(construction).unwrap();
    let after = lent(&world);
    assert_eq!(after.len(), 6);
    assert!(after.iter().any(|c| c.object == pending));
    assert_eq!(after, world.snapshot().components);
    assert!(world.published_objects().any(|o| o == pending));
    assert!(world.published_objects().eq(handles(&world)));
}

#[test]
fn follows_a_snapshot_after_a_removal_frees_a_slot() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let first = world.create_object(ObjectDesc::default()).unwrap();
    script(&mut world, first, "First");
    let doomed = world.create_object(ObjectDesc::default()).unwrap();
    let removed = world
        .add_component(
            doomed,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Doomed".into(),
            },
        )
        .unwrap();
    script(&mut world, first, "Second");
    world.remove_component(removed).unwrap();
    let freed = lent(&world);
    assert!(freed.iter().all(|c| c.handle != removed));
    assert_eq!(freed, world.snapshot().components);
    script(&mut world, doomed, "Reused");
    let reused = lent(&world);
    assert!(reused.iter().any(|c| c.object == doomed));
    assert_eq!(reused, world.snapshot().components);
}
