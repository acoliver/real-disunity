//! `removed_objects` counts the objects a world has removed, so a holder of handles can tell
//! when one might have stopped being live.
use worldrunner_world_simulation::*;

fn frame(world: &mut World) {
    let mut work = world.advance(TimeSpan::ZERO).unwrap();
    while let Advance::Callback(callback) = work {
        work = world
            .resume(callback.ticket, CallbackOutcome::Returned)
            .unwrap();
    }
}

#[test]
fn the_count_moves_only_when_objects_are_removed() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert_eq!(world.removed_objects(), 0);
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    let kept = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    frame(&mut world);
    assert_eq!(
        world.removed_objects(),
        0,
        "creating and updating removes nothing"
    );
    world.destroy(parent, DestroyWhen::EndOfFrame).unwrap();
    assert_eq!(
        world.removed_objects(),
        0,
        "destruction waits for the end of the frame"
    );
    assert!(world.object_alive(parent));
    frame(&mut world);
    assert_eq!(world.removed_objects(), 2, "the parent and its child");
    assert!(!world.object_alive(parent) && !world.object_alive(child));
    assert!(world.object_alive(kept));
    frame(&mut world);
    assert_eq!(world.removed_objects(), 2);
}
