use worldrunner_world_simulation::{
    Advance, ComponentDesc, DestroyWhen, ObjectDesc, ParentSpace, TimeSpan, World, WorldError,
    WorldSettings,
};

#[test]
fn discarding_construction_preserves_an_unrelated_pending_destroy() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let survivor = world.create_object(ObjectDesc::default()).unwrap();
    let pending = world.create_object(ObjectDesc::default()).unwrap();
    world.destroy(pending, DestroyWhen::EndOfFrame).unwrap();
    let before = world.storage_counts();
    let construction = world.begin_construction().unwrap();
    let created = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    let script = world
        .add_component(
            created,
            ComponentDesc::Script {
                assembly: "Game".into(),
                type_name: "Discarded".into(),
            },
        )
        .unwrap();
    world.discard_construction(construction).unwrap();
    assert_eq!(world.storage_counts(), before);
    assert!(world.object(created).is_err());
    assert!(world.component(script).is_err());
    assert!(world.object(pending).is_ok());
    assert!(world.object(survivor).is_ok());
    assert!(world.take_cleanup().is_empty());
    assert!(matches!(
        world.advance(TimeSpan::ZERO).unwrap(),
        Advance::Complete(_)
    ));
    assert!(world.object(pending).is_err());
    assert!(world.object(survivor).is_ok());
    assert_eq!(world.storage_counts(), (1, 1));
    assert_eq!(
        world.discard_construction(construction),
        Err(WorldError::WrongMutation)
    );
}

#[test]
fn constructing_under_a_pending_parent_inherits_its_retirement() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    world.destroy(parent, DestroyWhen::EndOfFrame).unwrap();
    let construction = world.begin_construction().unwrap();
    let child = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    let script = world
        .add_component(
            child,
            ComponentDesc::Script {
                assembly: "Game".into(),
                type_name: "PendingChild".into(),
            },
        )
        .unwrap();
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    assert_eq!(world.parent(child).unwrap(), Some(parent));
    assert!(world.object(parent).is_ok());
    assert!(world.object(child).is_ok());
    assert_eq!(
        world.activate_construction(construction).unwrap(),
        Advance::LifecycleComplete
    );
    assert!(!world.script_ready(script).unwrap());
    assert!(world.take_cleanup().is_empty());
    assert!(matches!(
        world.advance(TimeSpan::ZERO).unwrap(),
        Advance::Complete(_)
    ));
    assert!(world.object(parent).is_err());
    assert!(world.object(child).is_err());
    assert!(world.component(script).is_err());
    assert_eq!(world.storage_counts(), (0, 0));
}
