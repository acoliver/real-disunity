#[test]
fn callback_scale_change_applies_to_next_admission_not_remaining_fixed_steps() {
    let (mut world, _) = make_world();
    let mut work = world.advance(span(1, 25)).unwrap();
    let mut fixed = 0;
    loop {
        match work {
            Advance::Callback(c) => {
                if c.message == ScriptMessage::FixedUpdate {
                    fixed += 1;
                    world.set_time_scale(TimeSpan::ZERO).unwrap();
                    assert_eq!(world.clock().unscaled_delta, TimeSpan::FIXED);
                }
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(fixed, 2);
    assert_eq!(world.clock().time, span(1, 25));
    let work = world.advance(TimeSpan::FIXED).unwrap();
    finish(&mut world, work);
    assert_eq!(world.clock().fixed_frame, 2);
    assert_eq!(world.clock().time, span(1, 25));
}

#[test]
fn timeline_overflow_rejects_before_clock_or_input_mutation() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    world.set_max_delta(span(9_007_199_254_740_991, 1)).unwrap();
    let work = world.advance(span(9_007_199_254_740_991, 1)).unwrap();
    finish(&mut world, work);
    let before = world.clock().clone();
    let input = world.input(InputPhase::Dynamic).clone();
    assert_eq!(
        world.advance(TimeSpan::ONE),
        Err(WorldError::InvalidTime(TimeError::Overflow))
    );
    assert_eq!(world.clock(), &before);
    assert_eq!(world.input(InputPhase::Dynamic), &input);
}

use worldrunner_world_simulation::*;
fn span(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}
fn make_world() -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Actor".into(),
            },
        )
        .unwrap();
    (world, object)
}
fn finish(world: &mut World, mut work: Advance) {
    loop {
        match work {
            Advance::Callback(c) => {
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => return,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => panic!(),
        }
    }
}
#[test]
fn fixed_phase_has_fixed_delta_and_dynamic_restores_frame_delta() {
    let (mut world, _) = make_world();
    let mut work = world.advance(span(3, 100)).unwrap();
    loop {
        match work {
            Advance::Callback(c) => {
                if c.message == ScriptMessage::FixedUpdate {
                    assert_eq!(world.clock().delta, span(1, 50));
                    assert_eq!(world.clock().fixed_time, span(1, 50));
                }
                if c.message == ScriptMessage::Update {
                    assert_eq!(world.clock().delta, span(3, 100));
                }
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::AssetCallback(_) => panic!("unexpected asset callback"),
            Advance::LifecycleComplete => panic!(),
        }
    }
}
#[test]
fn delayed_destruction_uses_scaled_time_and_pause_preserves_it() {
    let (mut world, object) = make_world();
    let work = world.advance(TimeSpan::ZERO).unwrap();
    finish(&mut world, work);
    world
        .destroy(object, DestroyWhen::AfterScaled(span(1, 10)))
        .unwrap();
    world.set_time_scale(TimeSpan::ZERO).unwrap();
    let work = world.advance(span(1, 1)).unwrap();
    finish(&mut world, work);
    assert!(world.object(object).is_ok());
    world.set_time_scale(TimeSpan::ONE).unwrap();
    let work = world.advance(span(1, 10)).unwrap();
    finish(&mut world, work);
    assert!(world.object(object).is_err());
}
#[test]
fn keyboard_axis_projection_and_focus_release() {
    use std::collections::BTreeMap;
    let (mut world, _) = make_world();
    world
        .set_input_axes(BTreeMap::from([(
            AxisId(1),
            AxisBinding {
                negative: vec![KeyCode(1)],
                positive: vec![KeyCode(2)],
                invert: false,
            },
        )]))
        .unwrap();
    world
        .push_input(&[InputEvent {
            timestamp: TimeSpan::ZERO,
            action: InputAction::Key(KeyCode(2), true),
        }])
        .unwrap();
    let work = world.advance(span(1, 50)).unwrap();
    finish(&mut world, work);
    assert!((world.input(InputPhase::Dynamic).axes[&AxisId(1)] - 1.0).abs() < f32::EPSILON);
    world
        .push_input(&[InputEvent {
            timestamp: span(3, 100),
            action: InputAction::Focus(false),
        }])
        .unwrap();
    let work = world.advance(span(1, 50)).unwrap();
    finish(&mut world, work);
    assert!(world.input(InputPhase::Dynamic).axes[&AxisId(1)].abs() < f32::EPSILON);
}
#[test]
fn bad_input_batch_is_atomic_and_future_events_wait() {
    let (mut world, _) = make_world();
    assert_eq!(
        world.push_input(&[
            InputEvent {
                timestamp: span(2, 10),
                action: InputAction::Key(KeyCode(1), true)
            },
            InputEvent {
                timestamp: span(1, 10),
                action: InputAction::Focus(false)
            }
        ]),
        Err(WorldError::InputOrder)
    );
    world
        .push_input(&[InputEvent {
            timestamp: span(1, 10),
            action: InputAction::Key(KeyCode(2), true),
        }])
        .unwrap();
    let work = world.advance(span(1, 50)).unwrap();
    finish(&mut world, work);
    assert!(world.input(InputPhase::Dynamic).held.is_empty());
    let work = world.advance(span(8, 100)).unwrap();
    finish(&mut world, work);
    assert_eq!(
        world
            .input(InputPhase::Dynamic)
            .held
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![KeyCode(2)]
    );
}
#[test]
fn wrong_world_ticket_and_overlapping_advances_do_not_consume_callback() {
    let (mut a, _) = make_world();
    let (mut b, _) = make_world();
    let Advance::Callback(ca) = a.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    let Advance::Callback(cb) = b.advance(TimeSpan::ZERO).unwrap() else {
        panic!()
    };
    assert_eq!(
        a.advance(TimeSpan::ZERO),
        Err(WorldError::AdvanceInProgress)
    );
    assert_eq!(
        a.resume(cb.ticket, CallbackOutcome::Returned),
        Err(WorldError::WrongCallback)
    );
    assert_eq!(
        a.resume(
            ca.ticket,
            CallbackOutcome::Yielded(WaitCondition::NextFrame)
        ),
        Err(WorldError::WrongCallback)
    );
    let work = a.resume(ca.ticket, CallbackOutcome::Returned).unwrap();
    finish(&mut a, work);
}
