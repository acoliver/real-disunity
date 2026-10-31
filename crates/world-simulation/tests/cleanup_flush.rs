//! `flush_cleanup` runs the `OnDisable` and `OnDestroy` messages that destruction has queued, now.
use worldrunner_world_simulation::*;

fn script(world: &mut World) -> (ObjectHandle, ComponentHandle) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Actor".into(),
            },
        )
        .unwrap();
    (object, script)
}

fn drain_frame(world: &mut World) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut work = world.advance(TimeSpan::ZERO).unwrap();
    drain(world, &mut work)
}

fn drain(world: &mut World, work: &mut Advance) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut messages = Vec::new();
    loop {
        match *work {
            Advance::Callback(callback) => {
                messages.push((callback.script, callback.message));
                *work = world
                    .resume(callback.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::Complete(_) | Advance::LifecycleComplete => return messages,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
        }
    }
}

#[test]
fn flush_runs_the_queued_cleanup_of_a_destroyed_component_once() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, actor) = script(&mut world);
    drain_frame(&mut world);
    world.destroy_component(actor).unwrap();
    let mut work = world.flush_cleanup().unwrap();
    assert_eq!(
        drain(&mut world, &mut work),
        vec![
            (actor, ScriptMessage::OnDisable),
            (actor, ScriptMessage::OnDestroy)
        ]
    );
    assert_eq!(work, Advance::LifecycleComplete);
    assert!(world.component(actor).is_ok());
    assert_eq!(drain_frame(&mut world), vec![]);
    assert!(world.component(actor).is_err());
}

#[test]
fn flush_with_nothing_queued_completes_at_once() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    drain_frame(&mut world);
    assert_eq!(world.flush_cleanup().unwrap(), Advance::LifecycleComplete);
}

#[test]
fn flush_runs_the_cleanup_of_a_destroyed_object_inside_a_callback() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, actor) = script(&mut world);
    drain_frame(&mut world);
    let Advance::Callback(parent) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!("expected a callback");
    };
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    let mut work = world.flush_cleanup().unwrap();
    assert_eq!(
        drain(&mut world, &mut work),
        vec![
            (actor, ScriptMessage::OnDisable),
            (actor, ScriptMessage::OnDestroy)
        ]
    );
    let mut work = world
        .resume(parent.ticket, CallbackOutcome::Returned)
        .unwrap();
    let rest = drain(&mut world, &mut work);
    assert!(rest.iter().all(|(_, message)| !matches!(
        message,
        ScriptMessage::OnDisable | ScriptMessage::OnDestroy
    )));
}

#[test]
fn a_faulted_world_refuses_to_flush() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    let Advance::Callback(awake) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!("expected a callback");
    };
    let failure = ScriptFailure {
        message: "awake failed".into(),
    };
    let mut result = world.resume(awake.ticket, CallbackOutcome::Failed(failure));
    while let Ok(Advance::Callback(c)) = result {
        result = world.resume(c.ticket, CallbackOutcome::Returned);
    }
    assert_eq!(world.flush_cleanup(), Err(WorldError::Faulted));
}
