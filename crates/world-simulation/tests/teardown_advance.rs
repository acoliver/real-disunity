//! `advance_teardown` runs the queued `OnDisable` and `OnDestroy` messages and no frame phase.
use worldrunner_world_simulation::*;

type Seen = Vec<(ComponentHandle, ScriptMessage)>;

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

fn drain(world: &mut World, work: &mut Advance) -> Seen {
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

fn frame(world: &mut World, elapsed: TimeSpan) -> Seen {
    let mut work = world.advance(elapsed).unwrap();
    drain(world, &mut work)
}

fn teardown(world: &mut World) -> Seen {
    let mut work = world.advance_teardown().unwrap();
    drain(world, &mut work)
}

fn tick() -> TimeSpan {
    TimeSpan::new(1, 25).unwrap()
}

#[test]
fn teardown_runs_the_cleanup_of_destroyed_objects_and_no_frame_phase() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (gone, doomed) = script(&mut world);
    let (_, live) = script(&mut world);
    frame(&mut world, tick());
    world.destroy(gone, DestroyWhen::EndOfFrame).unwrap();
    let seen = teardown(&mut world);
    assert_eq!(
        seen,
        vec![
            (doomed, ScriptMessage::OnDisable),
            (doomed, ScriptMessage::OnDestroy)
        ]
    );
    assert!(seen.iter().all(|(script, _)| *script != live));
    assert!(world.component(doomed).is_err());
    assert!(world.component(live).is_ok());
}

#[test]
fn teardown_starts_no_script_that_has_not_started() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    frame(&mut world, tick());
    let (_, late) = script(&mut world);
    let seen = teardown(&mut world);
    assert!(
        seen.iter().all(|(script, _)| *script != late),
        "a teardown advance sent {seen:?}"
    );
}

#[test]
fn teardown_leaves_the_frame_count_and_time_unchanged() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, _) = script(&mut world);
    frame(&mut world, tick());
    frame(&mut world, tick());
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    let before = world.clock().clone();
    teardown(&mut world);
    assert_eq!(world.clock(), &before);
    assert_eq!(before.frame, 2);
}

#[test]
fn an_on_destroy_that_destroys_another_object_has_it_cleaned_up_in_the_same_advance() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (first, first_script) = script(&mut world);
    let (second, second_script) = script(&mut world);
    frame(&mut world, tick());
    world.destroy(first, DestroyWhen::EndOfFrame).unwrap();
    let mut seen = Vec::new();
    let mut work = world.advance_teardown().unwrap();
    while let Advance::Callback(callback) = work {
        seen.push((callback.script, callback.message));
        if (callback.script, callback.message) == (first_script, ScriptMessage::OnDestroy) {
            world.destroy(second, DestroyWhen::EndOfFrame).unwrap();
        }
        work = world
            .resume(callback.ticket, CallbackOutcome::Returned)
            .unwrap();
    }
    assert!(matches!(work, Advance::Complete(_)));
    assert_eq!(
        seen,
        vec![
            (first_script, ScriptMessage::OnDisable),
            (first_script, ScriptMessage::OnDestroy),
            (second_script, ScriptMessage::OnDisable),
            (second_script, ScriptMessage::OnDestroy),
        ]
    );
    assert!(world.component(second_script).is_err());
}

#[test]
fn teardown_with_nothing_queued_completes_without_callbacks() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    frame(&mut world, tick());
    assert_eq!(teardown(&mut world), vec![]);
}

#[test]
fn teardown_reports_no_fixed_step_after_a_frame_that_had_some() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut work = world.advance(TimeSpan::new(1, 5).unwrap()).unwrap();
    assert!(drain(&mut world, &mut work).is_empty());
    let Advance::Complete(before) = work else {
        panic!("expected a completed frame, got {work:?}");
    };
    assert!(
        before.fixed_steps > 0,
        "the frame ran fixed steps: {before:?}"
    );
    let mut work = world.advance_teardown().unwrap();
    assert!(drain(&mut world, &mut work).is_empty());
    assert_eq!(
        work,
        Advance::Complete(FrameSummary {
            frame: before.frame,
            fixed_steps: 0,
        }),
        "teardown runs no fixed step and keeps the frame count"
    );
}

#[test]
fn teardown_is_refused_while_another_advance_is_in_progress() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    let Advance::Callback(callback) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!("expected a callback");
    };
    assert_eq!(world.advance_teardown(), Err(WorldError::AdvanceInProgress));
    assert!(
        world
            .resume(callback.ticket, CallbackOutcome::Returned)
            .is_ok()
    );
}

#[test]
fn a_faulted_world_refuses_a_teardown_advance() {
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
    assert_eq!(world.advance_teardown(), Err(WorldError::Faulted));
}
