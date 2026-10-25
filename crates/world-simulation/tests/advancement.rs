use worldrunner_world_simulation::*;

fn time(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}
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
fn drain(world: &mut World, mut work: Advance) -> Vec<ScriptMessage> {
    let mut messages = Vec::new();
    loop {
        match work {
            Advance::Callback(callback) => {
                messages.push(callback.message);
                work = world
                    .resume(callback.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::Complete(_) => return messages,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => panic!("unexpected child scope"),
        }
    }
}
#[test]
fn fixed_dynamic_order_and_fractional_accumulator() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    let work = world.advance(time(3, 100)).unwrap();
    assert_eq!(
        drain(&mut world, work),
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start,
            ScriptMessage::FixedUpdate,
            ScriptMessage::Update,
            ScriptMessage::LateUpdate
        ]
    );
    let work = world.advance(time(1, 100)).unwrap();
    assert_eq!(
        drain(&mut world, work),
        vec![
            ScriptMessage::FixedUpdate,
            ScriptMessage::Update,
            ScriptMessage::LateUpdate
        ]
    );
    assert_eq!(world.clock().fixed_frame, 2);
}
#[test]
fn input_cutoffs_ties_focus_and_independent_edges() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    world
        .push_input(&[
            InputEvent {
                timestamp: time(1, 100),
                action: InputAction::Key(KeyCode(1), true),
            },
            InputEvent {
                timestamp: time(3, 100),
                action: InputAction::Key(KeyCode(2), true),
            },
            InputEvent {
                timestamp: time(3, 100),
                action: InputAction::Focus(false),
            },
        ])
        .unwrap();
    let mut work = world.advance(time(4, 100)).unwrap();
    let mut fixed = 0;
    loop {
        match work {
            Advance::Callback(c) => {
                if c.message == ScriptMessage::FixedUpdate {
                    fixed += 1;
                    let input = world.input(InputPhase::Fixed);
                    if fixed == 1 {
                        assert!(input.down.contains(&KeyCode(1)));
                        assert!(!input.down.contains(&KeyCode(2)));
                    } else {
                        assert!(input.held.is_empty());
                        assert!(input.up.contains(&KeyCode(1)));
                        assert!(!input.down.contains(&KeyCode(1)));
                    }
                }
                if c.message == ScriptMessage::Update {
                    let input = world.input(InputPhase::Dynamic);
                    assert!(input.held.is_empty());
                    assert!(input.down.contains(&KeyCode(1)));
                    assert!(input.up.contains(&KeyCode(2)));
                }
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(fixed, 2);
    assert_eq!(
        world.push_input(&[InputEvent {
            timestamp: time(4, 100),
            action: InputAction::Focus(true)
        }]),
        Err(WorldError::InputOrder)
    );
}
#[test]
fn pause_scale_waits_and_completion() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, s) = script(&mut world);
    let work = world.advance(TimeSpan::ZERO).unwrap();
    drain(&mut world, work);
    let coroutine = world
        .start_coroutine(s, WaitCondition::Unscaled(time(1, 10)))
        .unwrap();
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    let mut work = world.advance(time(1, 10)).unwrap();
    let mut resumed = false;
    loop {
        match work {
            Advance::Callback(c) => {
                assert_ne!(c.message, ScriptMessage::FixedUpdate);
                if c.message == ScriptMessage::Coroutine(coroutine) {
                    resumed = true;
                }
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert!(resumed);
    assert_eq!(world.coroutine_count(), 0);
    assert_eq!(world.clock().time, TimeSpan::ZERO);
    assert_eq!(world.clock().unscaled_time, time(1, 10));
    world.set_time_scale(time(1, 2)).unwrap();
    let work = world.advance(time(1, 10)).unwrap();
    drain(&mut world, work);
    assert_eq!(world.clock().time, time(1, 20));
}
#[test]
fn mutation_and_nested_clone_scopes_block_parent_resume() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, _) = script(&mut world);
    let Advance::Callback(parent) = world.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    let clone = world.instantiate(template, None).unwrap();
    let Advance::Callback(child) = world.next_lifecycle(clone.mutation).unwrap() else {
        panic!()
    };
    assert_eq!(child.message, ScriptMessage::Awake);
    assert_eq!(
        world.resume(parent.ticket, CallbackOutcome::Returned),
        Err(WorldError::WrongCallback)
    );
    let nested = world.instantiate(clone.root, None).unwrap();
    let nested_work = world.next_lifecycle(nested.mutation).unwrap();
    let mut work = nested_work;
    loop {
        match work {
            Advance::Callback(c) => {
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => break,
            Advance::Complete(_) => panic!(),
        }
    }
    let Advance::Callback(enable) = world
        .resume(child.ticket, CallbackOutcome::Returned)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(enable.message, ScriptMessage::OnEnable);
    assert_eq!(
        world
            .resume(enable.ticket, CallbackOutcome::Returned)
            .unwrap(),
        Advance::LifecycleComplete
    );
    world.set_active(template, false).unwrap();
    let work = world
        .resume(parent.ticket, CallbackOutcome::Returned)
        .unwrap();
    let messages = drain(&mut world, work);
    assert!(messages.contains(&ScriptMessage::OnDisable));
    assert_eq!(
        messages
            .iter()
            .filter(|&&m| m == ScriptMessage::Start)
            .count(),
        2
    );
}
#[test]
fn failure_tears_down_and_preserves_original_error() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    script(&mut world);
    let Advance::Callback(c) = world.advance(time(1, 100)).unwrap() else {
        panic!()
    };
    let failure = ScriptFailure {
        message: "awake failed".into(),
    };
    let mut result = world.resume(c.ticket, CallbackOutcome::Failed(failure.clone()));
    loop {
        match result {
            Ok(Advance::Callback(c)) => {
                assert!(matches!(
                    c.message,
                    ScriptMessage::OnDisable | ScriptMessage::OnDestroy
                ));
                result = world.resume(c.ticket, CallbackOutcome::Returned);
            }
            Err(WorldError::CallbackFailed(error)) => {
                assert_eq!(error, failure);
                break;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(world.storage_counts(), (0, 0));
    assert_eq!(world.advance(TimeSpan::ZERO), Err(WorldError::Faulted));
}
#[test]
fn excessive_catchup_and_overflow_are_atomic() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.configure_fixed_step(time(1, 10000), 2).unwrap();
    let before = world.clock().clone();
    assert!(matches!(
        world.advance(time(1, 1)),
        Err(WorldError::InvalidTime(_))
    ));
    assert_eq!(world.clock(), &before);
    assert!(TimeSpan::new(u64::MAX, 1).is_err());
    assert!(TimeSpan::new(1, 0).is_err());
}
#[test]
fn deterministic_message_trace() {
    let trace = || {
        let mut world = World::new(WorldSettings::default()).unwrap();
        script(&mut world);
        (0..50)
            .map(|_| {
                let work = world.advance(time(1, 120)).unwrap();
                drain(&mut world, work)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(trace(), trace());
}
