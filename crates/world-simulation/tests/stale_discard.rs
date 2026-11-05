//! Retiring by a handle whose object an earlier removal already took must not touch the object
//! that has since reused its slot.
use worldrunner_world_simulation::{
    Advance, CallbackOutcome, ComponentDesc, ComponentHandle, Construction, ObjectDesc,
    ObjectHandle, ParentSpace, TimeSpan, World, WorldSettings,
};

fn script(name: &str) -> ComponentDesc {
    ComponentDesc::Script {
        assembly: "Game".into(),
        type_name: name.into(),
    }
}

fn run_frame(world: &mut World) {
    let mut work = world.advance(TimeSpan::ZERO).unwrap();
    while let Advance::Callback(callback) = work {
        work = world
            .resume(callback.ticket, CallbackOutcome::Returned)
            .unwrap();
    }
}

struct Pair {
    outer: Construction,
    inner: Construction,
    inner_object: ObjectHandle,
}

/// Two open constructions whose second object hangs under the first's, so discarding the first
/// removes both.
fn nested_constructions(world: &mut World) -> Pair {
    let outer = world.begin_construction().unwrap();
    let outer_object = world
        .create_object_in(outer, ObjectDesc::default())
        .unwrap();
    let inner = world.begin_construction().unwrap();
    let inner_object = world
        .create_object_in(inner, ObjectDesc::default())
        .unwrap();
    world
        .set_parent(inner_object, Some(outer_object), ParentSpace::KeepLocal)
        .unwrap();
    Pair {
        outer,
        inner,
        inner_object,
    }
}

fn scripted_object(world: &mut World, name: &str) -> (ObjectHandle, ComponentHandle) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let component = world.add_component(object, script(name)).unwrap();
    world.set_script_enabled(component, true).unwrap();
    (object, component)
}

#[test]
fn discarding_a_construction_whose_object_an_earlier_discard_removed_spares_the_slot_reuser() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let pair = nested_constructions(&mut world);
    world.discard_construction(pair.outer).unwrap();
    assert!(!world.object_alive(pair.inner_object));
    let first = scripted_object(&mut world, "First");
    let second = scripted_object(&mut world, "Second");
    let removed = world.removed_objects();
    let counts = world.storage_counts();

    world.discard_construction(pair.inner).unwrap();

    assert!(world.object_alive(first.0) && world.object_alive(second.0));
    assert!(world.component(first.1).is_ok() && world.component(second.1).is_ok());
    assert_eq!(world.storage_counts(), counts);
    assert_eq!(world.removed_objects(), removed);
    run_frame(&mut world);
    assert!(world.script_ready(first.1).unwrap() && world.script_ready(second.1).unwrap());
}

#[test]
fn scripts_of_the_slot_reuser_keep_running_after_a_stale_discard() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let pair = nested_constructions(&mut world);
    world.discard_construction(pair.outer).unwrap();
    let first = scripted_object(&mut world, "First");
    let second = scripted_object(&mut world, "Second");
    world.discard_construction(pair.inner).unwrap();
    for (object, script) in [first, second] {
        assert_eq!(world.parent(object).unwrap(), None);
        assert_eq!(world.children(object).unwrap(), Vec::new());
        world.set_script_enabled(script, false).unwrap();
        world.set_script_enabled(script, true).unwrap();
        world.set_active(object, false).unwrap();
        world.set_active(object, true).unwrap();
    }
    run_frame(&mut world);
    assert!(world.script_ready(first.1).unwrap() && world.script_ready(second.1).unwrap());
}
