#[test]
fn nested_clone_peer_deactivation_defers_awake() {
    let (mut world, template, _) = initialized();
    let (peer_object, peer) = actor(&mut world);
    world
        .set_parent(peer_object, Some(template), ParentSpace::KeepLocal)
        .unwrap();
    let next = world.advance(TimeSpan::ZERO).unwrap();
    drain(&mut world, next);
    let Advance::Callback(parent) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    let clone = world.instantiate(template, None).unwrap();
    let clone_peer = clone.components[&peer];
    let Advance::Callback(awake) = world.next_lifecycle(clone.mutation).unwrap() else {
        panic!()
    };
    world
        .set_active(clone.objects[&peer_object], false)
        .unwrap();
    let next = world
        .resume(awake.ticket, CallbackOutcome::Returned)
        .unwrap();
    assert_eq!(
        drain(&mut world, next)
            .into_iter()
            .filter(|(s, _)| *s == clone_peer)
            .collect::<Vec<_>>(),
        vec![(clone_peer, ScriptMessage::OnDisable)]
    );
    let mutation = world.set_active(clone.objects[&peer_object], true).unwrap();
    let next = world.next_lifecycle(mutation).unwrap();
    assert_eq!(
        drain(&mut world, next),
        vec![
            (clone_peer, ScriptMessage::Awake),
            (clone_peer, ScriptMessage::OnEnable),
        ]
    );
    let next = world
        .resume(parent.ticket, CallbackOutcome::Returned)
        .unwrap();
    drain(&mut world, next);
}

#[test]
fn peer_reenable_before_awake_enters_awake_before_one_enable() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    actor(&mut world);
    let (_, peer) = actor(&mut world);
    let Advance::Callback(first) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    world.set_script_enabled(peer, false).unwrap();
    world.set_script_enabled(peer, true).unwrap();
    let next = world
        .resume(first.ticket, CallbackOutcome::Returned)
        .unwrap();
    assert_eq!(
        drain(&mut world, next)
            .into_iter()
            .filter(|(s, _)| *s == peer)
            .collect::<Vec<_>>(),
        vec![
            (peer, ScriptMessage::OnDisable),
            (peer, ScriptMessage::Awake),
            (peer, ScriptMessage::OnEnable),
            (peer, ScriptMessage::Start),
            (peer, ScriptMessage::Update),
            (peer, ScriptMessage::LateUpdate),
        ]
    );
}

#[test]
fn coroutine_failure_returns_current_and_waiting_ids() {
    let (mut world, _, script) = initialized();
    let current = world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    let waiting = world
        .start_coroutine(script, WaitCondition::Scaled(TimeSpan::ONE))
        .unwrap();
    let mut next = world.advance(TimeSpan::ZERO).unwrap();
    loop {
        let Advance::Callback(c) = next else { panic!() };
        if c.message == ScriptMessage::Coroutine(current) {
            let failure = ScriptFailure {
                message: "iterator failed".into(),
            };
            let mut result = world.resume(c.ticket, CallbackOutcome::Failed(failure.clone()));
            assert_eq!(world.take_cancelled_coroutines(), vec![current, waiting]);
            while let Ok(Advance::Callback(c)) = result {
                result = world.resume(c.ticket, CallbackOutcome::Returned);
            }
            assert_eq!(result, Err(WorldError::CallbackFailed(failure)));
            assert!(world.take_cancelled_coroutines().is_empty());
            break;
        }
        next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
    }
}

#[test]
fn stopped_suspended_coroutine_ignores_its_yield_deadline() {
    let (mut world, _, script) = initialized();
    let current = world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    let mut next = world.advance(TimeSpan::ONE).unwrap();
    loop {
        let Advance::Callback(c) = next else { panic!() };
        if c.message == ScriptMessage::Coroutine(current) {
            world.stop_coroutine(current).unwrap();
            assert_eq!(world.take_cancelled_coroutines(), vec![current]);
            let overflow = TimeSpan::new(9_007_199_254_740_991, 1).unwrap();
            next = world
                .resume(
                    c.ticket,
                    CallbackOutcome::Yielded(WaitCondition::Scaled(overflow)),
                )
                .unwrap();
            drain(&mut world, next);
            assert_eq!(world.coroutine_count(), 0);
            assert!(world.take_cancelled_coroutines().is_empty());
            break;
        }
        next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
    }
}

use worldrunner_world_simulation::*;

fn actor(world: &mut World) -> (ObjectHandle, ComponentHandle) {
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

fn drain(world: &mut World, mut next: Advance) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut trace = Vec::new();
    while let Advance::Callback(c) = next {
        trace.push((c.script, c.message));
        next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
    }
    trace
}

fn initialized() -> (World, ObjectHandle, ComponentHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, script) = actor(&mut world);
    let next = world.advance(TimeSpan::ZERO).unwrap();
    drain(&mut world, next);
    (world, object, script)
}

#[test]
fn awake_disable_reenable_delivers_one_enable() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = actor(&mut world);
    let Advance::Callback(awake) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    world.set_script_enabled(script, false).unwrap();
    world.set_script_enabled(script, true).unwrap();
    let next = world
        .resume(awake.ticket, CallbackOutcome::Returned)
        .unwrap();
    assert_eq!(
        drain(&mut world, next),
        vec![
            (script, ScriptMessage::OnDisable),
            (script, ScriptMessage::OnEnable),
            (script, ScriptMessage::Start),
            (script, ScriptMessage::Update),
            (script, ScriptMessage::LateUpdate),
        ]
    );
}

#[test]
fn peer_deactivated_before_awake_waits_for_reactivation() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    actor(&mut world);
    let (object, peer) = actor(&mut world);
    let Advance::Callback(first) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    world.set_active(object, false).unwrap();
    let next = world
        .resume(first.ticket, CallbackOutcome::Returned)
        .unwrap();
    let trace = drain(&mut world, next);
    assert_eq!(
        trace
            .into_iter()
            .filter(|(s, _)| *s == peer)
            .collect::<Vec<_>>(),
        vec![(peer, ScriptMessage::OnDisable)]
    );
    world.set_active(object, true).unwrap();
    let next = world.advance(TimeSpan::ZERO).unwrap();
    assert_eq!(
        drain(&mut world, next)
            .into_iter()
            .filter(|(s, _)| *s == peer)
            .collect::<Vec<_>>(),
        vec![
            (peer, ScriptMessage::Awake),
            (peer, ScriptMessage::OnEnable),
            (peer, ScriptMessage::Start),
            (peer, ScriptMessage::Update),
            (peer, ScriptMessage::LateUpdate),
        ]
    );
}

#[test]
fn nested_clone_activation_reconciles_pending_work() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, source) = actor(&mut world);
    let Advance::Callback(parent) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    let clone = world.instantiate(template, None).unwrap();
    let script = clone.components[&source];
    let Advance::Callback(awake) = world.next_lifecycle(clone.mutation).unwrap() else {
        panic!()
    };
    world.set_script_enabled(script, false).unwrap();
    world.set_script_enabled(script, true).unwrap();
    let next = world
        .resume(awake.ticket, CallbackOutcome::Returned)
        .unwrap();
    assert_eq!(
        drain(&mut world, next),
        vec![
            (script, ScriptMessage::OnDisable),
            (script, ScriptMessage::OnEnable),
        ]
    );
    let next = world
        .resume(parent.ticket, CallbackOutcome::Returned)
        .unwrap();
    drain(&mut world, next);
}

fn cleanup_failure(first_message: ScriptMessage) {
    let (mut world, first_object, first_script) = initialized();
    let (second_object, second_script) = actor(&mut world);
    let next = world.advance(TimeSpan::ZERO).unwrap();
    drain(&mut world, next);
    world
        .destroy(first_object, DestroyWhen::EndOfFrame)
        .unwrap();
    world
        .destroy(second_object, DestroyWhen::EndOfFrame)
        .unwrap();
    let original = ScriptFailure {
        message: "first cleanup failure".into(),
    };
    let mut next = world.advance(TimeSpan::ZERO);
    let mut trace = Vec::new();
    let mut failed = false;
    loop {
        match next {
            Ok(Advance::Callback(c)) => {
                trace.push((c.script, c.message));
                let outcome = if !failed && c.message == first_message {
                    failed = true;
                    CallbackOutcome::Failed(original.clone())
                } else {
                    CallbackOutcome::Returned
                };
                next = world.resume(c.ticket, outcome);
            }
            Err(WorldError::CallbackFailed(failure)) => {
                assert_eq!(failure, original);
                break;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        trace,
        vec![
            (first_script, ScriptMessage::OnDisable),
            (first_script, ScriptMessage::OnDestroy),
            (second_script, ScriptMessage::OnDisable),
            (second_script, ScriptMessage::OnDestroy),
        ]
    );
    assert_eq!(world.storage_counts(), (0, 0));
    assert_eq!(world.advance(TimeSpan::ZERO), Err(WorldError::Faulted));
}

#[test]
fn first_failure_in_disable_preserves_remaining_cleanup() {
    cleanup_failure(ScriptMessage::OnDisable);
}

#[test]
fn first_failure_in_destroy_preserves_remaining_cleanup() {
    cleanup_failure(ScriptMessage::OnDestroy);
}

#[test]
fn intermediate_fixed_overflow_rejects_without_publishing() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let p = 999_999_999_999;
    world
        .configure_fixed_step(TimeSpan::new(p, 1_000_000_000_000).unwrap(), 1024)
        .unwrap();
    world
        .set_time_scale(TimeSpan::new(p, 500_000_000_000).unwrap())
        .unwrap();
    for seconds in [512, 512, 512, 512, 512, 512, 512, 512, 408] {
        let next = world.advance(TimeSpan::new(seconds, 1).unwrap()).unwrap();
        drain(&mut world, next);
    }
    assert_eq!(world.clock().fixed_frame, 9008);
    world
        .push_input(&[InputEvent {
            timestamp: TimeSpan::new(9009, 2).unwrap(),
            action: InputAction::Key(KeyCode(7), true),
        }])
        .unwrap();
    let before = world.clock().clone();
    let fixed = world.input(InputPhase::Fixed).clone();
    let dynamic = world.input(InputPhase::Dynamic).clone();
    assert_eq!(
        world.advance(TimeSpan::ONE),
        Err(WorldError::InvalidTime(TimeError::Overflow))
    );
    assert_eq!(world.clock(), &before);
    assert_eq!(world.input(InputPhase::Fixed), &fixed);
    assert_eq!(world.input(InputPhase::Dynamic), &dynamic);
    let next = world.advance(TimeSpan::ZERO).unwrap();
    drain(&mut world, next);
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    let next = world.advance(TimeSpan::ONE).unwrap();
    drain(&mut world, next);
    assert!(world.input(InputPhase::Dynamic).down.contains(&KeyCode(7)));
}

#[test]
fn cancellation_returns_ids_once_for_each_native_cause() {
    for cause in 0..5 {
        let (mut world, object, script) = initialized();
        let parent = world.create_object(ObjectDesc::default()).unwrap();
        world
            .set_parent(object, Some(parent), ParentSpace::KeepLocal)
            .unwrap();
        let first = world
            .start_coroutine(script, WaitCondition::NextFrame)
            .unwrap();
        let second = world
            .start_coroutine(script, WaitCondition::NextFrame)
            .unwrap();
        match cause {
            0 => {
                world.set_active(object, false).unwrap();
            }
            1 => {
                world.set_active(parent, false).unwrap();
            }
            2 => {
                world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
            }
            3 => {
                world.stop_coroutine(first).unwrap();
                world.stop_coroutine(first).unwrap();
                world.stop_coroutine(second).unwrap();
            }
            4 => {
                let Advance::Callback(c) = world.advance(TimeSpan::ZERO).unwrap() else {
                    panic!()
                };
                let mut next = world.resume(
                    c.ticket,
                    CallbackOutcome::Failed(ScriptFailure {
                        message: "update".into(),
                    }),
                );
                while let Ok(Advance::Callback(c)) = next {
                    next = world.resume(c.ticket, CallbackOutcome::Returned);
                }
                assert!(matches!(next, Err(WorldError::CallbackFailed(_))));
            }
            _ => unreachable!(),
        }
        assert_eq!(world.coroutine_count(), 0);
        assert_eq!(
            world.take_cancelled_coroutines(),
            vec![first, second],
            "cause {cause}"
        );
        assert!(world.take_cancelled_coroutines().is_empty());
    }
}

#[test]
fn suspended_coroutine_cancellation_notifies_without_resurrection() {
    let (mut world, object, script) = initialized();
    let id = world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    let mut next = world.advance(TimeSpan::ZERO).unwrap();
    loop {
        let Advance::Callback(c) = next else {
            panic!("coroutine was not dispatched")
        };
        if c.message == ScriptMessage::Coroutine(id) {
            world.set_active(object, false).unwrap();
            assert_eq!(world.take_cancelled_coroutines(), vec![id]);
            next = world
                .resume(c.ticket, CallbackOutcome::Yielded(WaitCondition::NextFrame))
                .unwrap();
            drain(&mut world, next);
            break;
        }
        next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
    }
    assert_eq!(world.coroutine_count(), 0);
    assert!(world.take_cancelled_coroutines().is_empty());
}
