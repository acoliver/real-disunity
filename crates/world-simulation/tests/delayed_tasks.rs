use worldrunner_world_simulation::*;

/// The callbacks of one frame, as `(identity or 0, message)`: scripts report 0, the
/// delayed-tasks receiver reports its identity.
fn frame(world: &mut World) -> Vec<(u64, ScriptMessage)> {
    let mut calls = Vec::new();
    let mut work = world.advance(TimeSpan::new(1, 100).unwrap()).unwrap();
    loop {
        work = match work {
            Advance::Callback(c) => {
                calls.push((0, c.message));
                world.resume(c.ticket, CallbackOutcome::Returned).unwrap()
            }
            Advance::AssetCallback(c) => {
                calls.push((c.identity, c.message));
                world.resume(c.ticket, CallbackOutcome::Returned).unwrap()
            }
            Advance::Complete(_) => return calls,
            Advance::LifecycleComplete => panic!("unexpected lifecycle completion"),
        };
    }
}

fn world_with_script() -> World {
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
    world
}

#[test]
fn the_receiver_gets_update_each_frame_between_update_and_late_update() {
    let mut world = world_with_script();
    frame(&mut world);
    world.set_delayed_tasks(Some(77)).unwrap();
    for _ in 0..2 {
        let calls = frame(&mut world);
        let update = calls
            .iter()
            .position(|&call| call == (0, ScriptMessage::Update))
            .expect("Update runs");
        let late = calls
            .iter()
            .position(|&call| call == (0, ScriptMessage::LateUpdate))
            .expect("LateUpdate runs");
        let tasks: Vec<_> = calls
            .iter()
            .enumerate()
            .filter(|(_, call)| call.0 == 77 && call.1 == ScriptMessage::Update)
            .collect();
        assert_eq!(tasks.len(), 1, "{calls:?}");
        let (at, &(_, message)) = tasks[0];
        assert_eq!(message, ScriptMessage::Update);
        assert!(update < at && at < late, "{calls:?}");
    }
    world.set_delayed_tasks(None).unwrap();
    assert!(frame(&mut world).iter().all(|call| call.0 == 0));
}

#[test]
fn the_receiver_gets_one_late_update_after_the_scripts_late_update_each_frame() {
    let mut world = world_with_script();
    frame(&mut world);
    world.set_delayed_tasks(Some(77)).unwrap();
    for _ in 0..2 {
        let calls = frame(&mut world);
        let late = calls
            .iter()
            .position(|&call| call == (0, ScriptMessage::LateUpdate))
            .expect("LateUpdate runs");
        let will_render: Vec<_> = calls
            .iter()
            .enumerate()
            .filter(|(_, call)| call.0 == 77 && call.1 == ScriptMessage::LateUpdate)
            .collect();
        assert_eq!(will_render.len(), 1, "{calls:?}");
        assert!(late < will_render[0].0, "{calls:?}");
    }
}

#[test]
fn the_receiver_runs_in_a_world_without_scripts_and_only_while_idle() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.set_delayed_tasks(Some(5)).unwrap();
    assert_eq!(
        frame(&mut world),
        [(5, ScriptMessage::Update), (5, ScriptMessage::LateUpdate)]
    );
    let mut scripted = world_with_script();
    let mut work = scripted.advance(TimeSpan::new(1, 100).unwrap()).unwrap();
    let Advance::Callback(callback) = work else {
        panic!("a script callback suspends the frame")
    };
    assert!(scripted.set_delayed_tasks(Some(5)).is_err());
    work = scripted
        .resume(callback.ticket, CallbackOutcome::Returned)
        .unwrap();
    while let Advance::Callback(c) = work {
        work = scripted
            .resume(c.ticket, CallbackOutcome::Returned)
            .unwrap();
    }
    assert!(matches!(work, Advance::Complete(_)));
}

#[test]
fn the_receiver_cannot_share_an_identity_with_a_script_asset() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert!(matches!(
        world.set_delayed_tasks(Some(0)),
        Err(WorldError::InvalidObject)
    ));
    world.create_script_asset(5).unwrap();
    assert!(matches!(
        world.set_delayed_tasks(Some(5)),
        Err(WorldError::InvalidObject)
    ));
    world.set_delayed_tasks(Some(6)).unwrap();
    assert!(matches!(
        world.create_script_asset(6),
        Err(WorldError::InvalidObject)
    ));
}

#[test]
fn a_failed_delayed_tasks_callback_tears_the_world_down_before_late_update() {
    let mut world = world_with_script();
    frame(&mut world);
    world.set_delayed_tasks(Some(77)).unwrap();
    let failure = ScriptFailure {
        message: "posted work failed".into(),
    };
    let mut calls = Vec::new();
    let mut result = world.advance(TimeSpan::new(1, 100).unwrap());
    loop {
        result = match result {
            Ok(Advance::Callback(c)) => {
                calls.push(c.message);
                world.resume(c.ticket, CallbackOutcome::Returned)
            }
            Ok(Advance::AssetCallback(c)) if c.message == ScriptMessage::Update => {
                world.resume(c.ticket, CallbackOutcome::Failed(failure.clone()))
            }
            Err(WorldError::CallbackFailed(error)) => {
                assert_eq!(error, failure);
                break;
            }
            other => panic!("{other:?}"),
        };
    }
    let late = calls.iter().position(|&m| m == ScriptMessage::LateUpdate);
    let disable = calls.iter().position(|&m| m == ScriptMessage::OnDisable);
    assert!(late.is_none() && disable.is_some(), "{calls:?}");
    assert_eq!(world.advance(TimeSpan::ZERO), Err(WorldError::Faulted));
}
