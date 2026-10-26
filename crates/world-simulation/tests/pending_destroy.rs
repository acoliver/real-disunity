#[test]
fn immediate_destroy_overrides_a_scaled_deadline() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .destroy(object, DestroyWhen::AfterScaled(TimeSpan::ONE))
        .unwrap();
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    assert!(world.object(object).is_ok());
    assert!(matches!(
        world.advance(TimeSpan::ZERO).unwrap(),
        Advance::Complete(_)
    ));
    assert!(world.object(object).is_err());
}

#[test]
fn pending_hierarchy_mutations_do_not_leave_dangling_owners() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    world.destroy(parent, DestroyWhen::EndOfFrame).unwrap();
    let added = world
        .add_component(
            parent,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Ball".into(),
            },
        )
        .unwrap();
    let mutation = world
        .set_parent(
            child,
            Some(parent),
            worldrunner_world_simulation::ParentSpace::KeepLocal,
        )
        .unwrap();
    assert!(world.take_lifecycle_work(mutation).unwrap().is_empty());
    assert!(world.component(added).is_ok());
    assert!(matches!(
        world.advance(TimeSpan::ZERO).unwrap(),
        Advance::Complete(_)
    ));
    assert!(world.component(added).is_err());
    assert!(world.object(child).is_err());
    assert!(world.snapshot().objects.is_empty());
    assert_eq!(world.storage_counts(), (0, 0));
}

use worldrunner_world_simulation::{
    Advance, ComponentDesc, DestroyWhen, ObjectDesc, ScriptMessage, TimeSpan, World, WorldSettings,
};

#[test]
fn pending_subtree_is_readable_writable_and_searchable_until_frame_completion() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Ball".into(),
            },
        )
        .unwrap();
    let mutation = world.set_active(object, true).unwrap();
    world.take_lifecycle_work(mutation).unwrap();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    assert!(world.object(object).is_ok());
    assert!(world.component(script).is_ok());
    assert!(!world.script_ready(script).unwrap());
    assert!(!world.coroutine_eligible(script));
    world.set_object_text(object, true, "Ball".into()).unwrap();
    assert_eq!(world.snapshot().objects.len(), 1);
    let mut advance = world.advance(TimeSpan::ZERO).unwrap();
    let mut messages = Vec::new();
    while let Advance::Callback(callback) = advance {
        messages.push(callback.message);
        advance = world
            .resume(
                callback.ticket,
                worldrunner_world_simulation::CallbackOutcome::Returned,
            )
            .unwrap();
    }
    assert_eq!(
        messages,
        [ScriptMessage::OnDisable, ScriptMessage::OnDestroy]
    );
    assert!(world.object(object).is_err());
    assert!(world.component(script).is_err());
    assert!(world.snapshot().objects.is_empty());
    assert!(world.set_object_text(object, true, "stale".into()).is_err());
}

#[test]
fn delayed_destruction_uses_scaled_time_and_coalesces_deadlines() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world.set_time_scale(TimeSpan::new(1, 2).unwrap()).unwrap();
    world
        .destroy(
            object,
            DestroyWhen::AfterScaled(TimeSpan::new(1, 10).unwrap()),
        )
        .unwrap();
    world
        .destroy(object, DestroyWhen::AfterScaled(TimeSpan::ONE))
        .unwrap();
    assert!(matches!(
        world.advance(TimeSpan::new(1, 10).unwrap()).unwrap(),
        Advance::Complete(_)
    ));
    assert!(world.object(object).is_ok());
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    world.advance(TimeSpan::ONE).unwrap();
    assert!(world.object(object).is_ok());
    world.set_time_scale(TimeSpan::new(1, 2).unwrap()).unwrap();
    world.advance(TimeSpan::new(1, 10).unwrap()).unwrap();
    assert!(world.object(object).is_err());
}
