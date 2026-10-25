#[test]
fn component_disable_preserves_coroutine_but_object_deactivation_cancels_it() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, script) = actor(&mut world, true);
    let work = world.advance(TimeSpan::ZERO).unwrap();
    run(&mut world, work);
    let id = world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    world.set_script_enabled(script, false).unwrap();
    assert_eq!(world.coroutine_count(), 1);
    let work = world.advance(TimeSpan::ZERO).unwrap();
    let trace = run(&mut world, work);
    assert!(trace.contains(&(script, ScriptMessage::Coroutine(id))));
    assert!(!trace.contains(&(script, ScriptMessage::Update)));
    world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    world.set_active(object, false).unwrap();
    assert_eq!(world.coroutine_count(), 0);
}

#[test]
fn late_destruction_cancels_waiting_coroutine_before_frame_completes() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, script) = actor(&mut world, true);
    let work = world.advance(TimeSpan::ZERO).unwrap();
    run(&mut world, work);
    world
        .start_coroutine(script, WaitCondition::Scaled(TimeSpan::ONE))
        .unwrap();
    let mut work = world.advance(TimeSpan::ZERO).unwrap();
    loop {
        match work {
            Advance::Callback(c) => {
                if c.message == ScriptMessage::LateUpdate {
                    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
                }
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(world.coroutine_count(), 0);
}

#[test]
fn child_can_destroy_suspended_parent_without_resurrecting_ordinary_work() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, parent_script) = actor(&mut world, true);
    let Advance::Callback(parent) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    let clone = world.instantiate(template, None).unwrap();
    let Advance::Callback(child) = world.next_lifecycle(clone.mutation).unwrap() else {
        panic!()
    };
    world.destroy(template, DestroyWhen::EndOfFrame).unwrap();
    let Advance::Callback(enable) = world
        .resume(child.ticket, CallbackOutcome::Returned)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        world
            .resume(enable.ticket, CallbackOutcome::Returned)
            .unwrap(),
        Advance::LifecycleComplete
    );
    let work = world
        .resume(parent.ticket, CallbackOutcome::Returned)
        .unwrap();
    let trace = run(&mut world, work);
    let parent_messages: Vec<_> = trace
        .into_iter()
        .filter(|(script, _)| *script == parent_script)
        .map(|(_, message)| message)
        .collect();
    assert_eq!(
        parent_messages,
        vec![ScriptMessage::OnDisable, ScriptMessage::OnDestroy]
    );
    assert_eq!(world.storage_counts().0, 1);
}

#[test]
fn clone_lifecycle_can_drain_without_advancing_time() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, _) = actor(&mut world, true);
    let clone = world.instantiate(template, None).unwrap();
    let mut work = world.next_lifecycle(clone.mutation).unwrap();
    let mut messages = Vec::new();
    loop {
        match work {
            Advance::Callback(c) => {
                messages.push(c.message);
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::LifecycleComplete => break,
            Advance::Complete(_) => panic!(),
        }
    }
    assert_eq!(
        messages,
        vec![ScriptMessage::Awake, ScriptMessage::OnEnable]
    );
    assert_eq!(world.clock().frame, 0);
    let work = world.advance(TimeSpan::ZERO).unwrap();
    let trace = run(&mut world, work);
    assert_eq!(
        trace
            .iter()
            .filter(|(_, message)| *message == ScriptMessage::Start)
            .count(),
        2
    );
}

use worldrunner_world_simulation::*;
fn span(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}
fn actor(world: &mut World, active: bool) -> (ObjectHandle, ComponentHandle) {
    let object = world
        .create_object(ObjectDesc {
            active,
            ..Default::default()
        })
        .unwrap();
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
fn run(world: &mut World, mut next: Advance) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut result = Vec::new();
    loop {
        match next {
            Advance::Callback(c) => {
                result.push((c.script, c.message));
                next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => return result,
            Advance::LifecycleComplete => panic!(),
        }
    }
}
#[test]
fn disabled_queued_start_is_not_lost_and_inactive_clone_waits() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, first) = actor(&mut world, true);
    let (_, second) = actor(&mut world, true);
    let mut next = world.advance(TimeSpan::ZERO).unwrap();
    loop {
        match next {
            Advance::Callback(c) => {
                if c.script == first && c.message == ScriptMessage::Start {
                    world.set_script_enabled(second, false).unwrap();
                }
                assert!(!(c.script == second && c.message == ScriptMessage::Start));
                next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
    world.set_script_enabled(second, true).unwrap();
    let next = world.advance(TimeSpan::ZERO).unwrap();
    assert!(run(&mut world, next).contains(&(second, ScriptMessage::Start)));
    let (template, _) = actor(&mut world, false);
    let clone = world.instantiate(template, None).unwrap();
    let next = world.advance(TimeSpan::ZERO).unwrap();
    assert!(
        !run(&mut world, next)
            .iter()
            .any(|(s, _)| clone.components.values().any(|c| c == s))
    );
    world.set_active(clone.root, true).unwrap();
    let next = world.advance(TimeSpan::ZERO).unwrap();
    let trace = run(&mut world, next);
    let clone_messages: Vec<_> = trace
        .into_iter()
        .filter(|(s, _)| clone.components.values().any(|c| c == s))
        .map(|(_, m)| m)
        .collect();
    assert_eq!(
        clone_messages,
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start,
            ScriptMessage::Update,
            ScriptMessage::LateUpdate
        ]
    );
}
#[test]
fn scripts_added_during_update_get_lifecycle_and_start_before_update() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    actor(&mut world, true);
    let mut next = world.advance(TimeSpan::ZERO).unwrap();
    let mut added = None;
    let mut trace = Vec::new();
    loop {
        match next {
            Advance::Callback(c) => {
                if c.message == ScriptMessage::Update && added.is_none() {
                    added = Some(actor(&mut world, true).1);
                }
                if Some(c.script) == added {
                    trace.push(c.message);
                }
                next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(
        trace,
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start,
            ScriptMessage::LateUpdate
        ]
    );
}
#[test]
fn destroy_during_update_suppresses_later_callbacks_and_cancels_iterator() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, s) = actor(&mut world, true);
    let next = world.advance(TimeSpan::ZERO).unwrap();
    run(&mut world, next);
    world.start_coroutine(s, WaitCondition::NextFrame).unwrap();
    let mut next = world.advance(TimeSpan::ZERO).unwrap();
    let mut messages = Vec::new();
    loop {
        match next {
            Advance::Callback(c) => {
                messages.push(c.message);
                if c.message == ScriptMessage::Update {
                    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
                }
                next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(
        messages,
        vec![
            ScriptMessage::Update,
            ScriptMessage::OnDisable,
            ScriptMessage::OnDestroy
        ]
    );
    assert_eq!(world.coroutine_count(), 0);
    assert_eq!(world.storage_counts(), (0, 0));
}
#[test]
fn child_awake_failure_cancels_parent_and_cleanup_failure_keeps_first_error() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, _) = actor(&mut world, true);
    let Advance::Callback(parent) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    let clone = world.instantiate(template, None).unwrap();
    let Advance::Callback(child) = world.next_lifecycle(clone.mutation).unwrap() else {
        panic!()
    };
    let original = ScriptFailure {
        message: "child awake".into(),
    };
    let mut next = world.resume(child.ticket, CallbackOutcome::Failed(original.clone()));
    let mut destroys = 0;
    loop {
        match next {
            Ok(Advance::Callback(c)) => {
                if c.message == ScriptMessage::OnDestroy {
                    destroys += 1;
                }
                assert_eq!(
                    world.create_object(ObjectDesc::default()),
                    Err(WorldError::Faulted)
                );
                next = world.resume(
                    c.ticket,
                    CallbackOutcome::Failed(ScriptFailure {
                        message: "cleanup".into(),
                    }),
                );
            }
            Err(WorldError::CallbackFailed(f)) => {
                assert_eq!(f, original);
                break;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(destroys, 2);
    assert_eq!(
        world.resume(parent.ticket, CallbackOutcome::Returned),
        Err(WorldError::WrongCallback)
    );
    assert_eq!(world.storage_counts(), (0, 0));
}
#[test]
fn coroutine_yield_scaled_wait_pause_and_fixed_resume() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, s) = actor(&mut world, true);
    let next = world.advance(TimeSpan::ZERO).unwrap();
    run(&mut world, next);
    let id = world
        .start_coroutine(s, WaitCondition::Scaled(span(1, 50)))
        .unwrap();
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    let next = world.advance(span(1, 10)).unwrap();
    assert!(!run(&mut world, next).contains(&(s, ScriptMessage::Coroutine(id))));
    world.set_time_scale(TimeSpan::ONE).unwrap();
    let mut next = world.advance(span(1, 50)).unwrap();
    loop {
        match next {
            Advance::Callback(c) => {
                let outcome = if c.message == ScriptMessage::Coroutine(id) {
                    CallbackOutcome::Yielded(WaitCondition::FixedUpdate)
                } else {
                    CallbackOutcome::Returned
                };
                next = world.resume(c.ticket, outcome).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(world.coroutine_count(), 1);
    let next = world.advance(span(1, 50)).unwrap();
    let trace = run(&mut world, next);
    let fixed = trace
        .iter()
        .position(|(_, m)| *m == ScriptMessage::FixedUpdate)
        .unwrap();
    let resumed = trace
        .iter()
        .position(|(_, m)| *m == ScriptMessage::Coroutine(id))
        .unwrap();
    assert!(fixed < resumed);
    assert_eq!(world.coroutine_count(), 0);
}
#[test]
fn fixed_update_runs_before_motion_and_update_observes_completed_motion() {
    use worldrunner_world_simulation::physics2d::{Body2d, BodyType};
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, _) = actor(&mut world, true);
    world
        .add_component(
            object,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Kinematic,
                velocity: glam::Vec2::X,
                ..Default::default()
            }),
        )
        .unwrap();
    let mut next = world.advance(span(1, 50)).unwrap();
    loop {
        match next {
            Advance::Callback(c) => {
                let x = world.local_transform(object).unwrap().translation.x;
                if c.message == ScriptMessage::FixedUpdate {
                    assert!(x.abs() < f32::EPSILON);
                }
                if c.message == ScriptMessage::Update {
                    assert!((x - 0.02).abs() < 1e-6);
                }
                next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
}
