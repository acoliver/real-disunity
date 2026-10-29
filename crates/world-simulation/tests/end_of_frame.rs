use worldrunner_world_simulation::*;

fn time(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}

fn world_with_script() -> (World, ComponentHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
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
    let first = world.advance(TimeSpan::ZERO).unwrap();
    frame(&mut world, first, |_| CallbackOutcome::Returned);
    (world, script)
}

/// Runs the frame, answering each coroutine step with `answer`, and returns the messages in order.
fn frame(
    world: &mut World,
    mut work: Advance,
    mut answer: impl FnMut(CoroutineId) -> CallbackOutcome,
) -> Vec<ScriptMessage> {
    let mut messages = Vec::new();
    loop {
        match work {
            Advance::Callback(callback) => {
                messages.push(callback.message);
                let outcome = match callback.message {
                    ScriptMessage::Coroutine(id) => answer(id),
                    _ => CallbackOutcome::Returned,
                };
                work = world.resume(callback.ticket, outcome).unwrap();
            }
            Advance::Complete(_) => return messages,
            other => panic!("unexpected {other:?}"),
        }
    }
}

fn step(
    world: &mut World,
    answer: impl FnMut(CoroutineId) -> CallbackOutcome,
) -> Vec<ScriptMessage> {
    let work = world.advance(time(1, 60)).unwrap();
    frame(world, work, answer)
}

#[test]
fn an_end_of_frame_wait_resumes_after_late_update_in_the_same_frame() {
    let (mut world, script) = world_with_script();
    let id = world
        .start_coroutine(script, WaitCondition::EndOfFrame)
        .unwrap();
    let messages = step(&mut world, |_| CallbackOutcome::Returned);
    let late = messages
        .iter()
        .position(|m| *m == ScriptMessage::LateUpdate)
        .expect("late update ran");
    let resumed = messages
        .iter()
        .position(|m| *m == ScriptMessage::Coroutine(id))
        .expect("the coroutine resumed");
    assert!(resumed > late, "{messages:?}");
    assert_eq!(world.coroutine_count(), 0);
}

#[test]
fn an_end_of_frame_wait_resumes_once_and_only_after_update() {
    let (mut world, script) = world_with_script();
    let id = world
        .start_coroutine(script, WaitCondition::EndOfFrame)
        .unwrap();
    let messages = step(&mut world, |_| CallbackOutcome::Returned);
    let update = messages
        .iter()
        .position(|m| *m == ScriptMessage::Update)
        .expect("update ran");
    let resumed = messages
        .iter()
        .position(|m| *m == ScriptMessage::Coroutine(id))
        .expect("the coroutine resumed");
    assert!(resumed > update, "{messages:?}");
    let count = messages
        .iter()
        .filter(|m| **m == ScriptMessage::Coroutine(id))
        .count();
    assert_eq!(count, 1, "{messages:?}");
}

#[test]
fn a_yield_made_at_the_end_of_a_frame_resumes_once_at_the_end_of_the_next_frame() {
    let (mut world, script) = world_with_script();
    let id = world
        .start_coroutine(script, WaitCondition::EndOfFrame)
        .unwrap();
    let first = step(&mut world, |_| {
        CallbackOutcome::Yielded(WaitCondition::EndOfFrame)
    });
    assert_eq!(resumes(&first, id), 1, "{first:?}");
    let second = step(&mut world, |_| CallbackOutcome::Returned);
    assert_eq!(resumes(&second, id), 1, "{second:?}");
    assert_eq!(second.last(), Some(&ScriptMessage::Coroutine(id)));
    assert_eq!(world.coroutine_count(), 0);
}

#[test]
fn an_update_coroutine_that_yields_end_of_frame_resumes_after_late_update_of_that_frame() {
    let (mut world, script) = world_with_script();
    let id = world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    let messages = step(&mut world, |_| {
        CallbackOutcome::Yielded(WaitCondition::EndOfFrame)
    });
    let steps: Vec<_> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| **m == ScriptMessage::Coroutine(id))
        .map(|(i, _)| i)
        .collect();
    let late = messages
        .iter()
        .position(|m| *m == ScriptMessage::LateUpdate)
        .expect("late update ran");
    assert_eq!(steps.len(), 2, "{messages:?}");
    assert!(steps[0] < late && steps[1] > late, "{messages:?}");
}

fn resumes(messages: &[ScriptMessage], id: CoroutineId) -> usize {
    messages
        .iter()
        .filter(|m| **m == ScriptMessage::Coroutine(id))
        .count()
}

#[test]
fn a_next_frame_wait_resumes_after_update_and_not_in_the_fixed_steps_before_it() {
    let (mut world, script) = world_with_script();
    let id = world
        .start_coroutine(script, WaitCondition::NextFrame)
        .unwrap();
    // A tenth of a second runs several fixed steps before the frame's Update.
    let work = world.advance(time(1, 10)).unwrap();
    let messages = frame(&mut world, work, |_| CallbackOutcome::Returned);
    let fixed = messages
        .iter()
        .rposition(|m| *m == ScriptMessage::FixedUpdate)
        .expect("fixed steps ran");
    let update = messages
        .iter()
        .position(|m| *m == ScriptMessage::Update)
        .expect("update ran");
    let resumed = messages
        .iter()
        .position(|m| *m == ScriptMessage::Coroutine(id))
        .expect("the coroutine resumed");
    assert!(fixed < update && update < resumed, "{messages:?}");
}
