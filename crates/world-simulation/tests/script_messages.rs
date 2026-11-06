//! Per-frame messages reach only the scripts whose type defines them.

use worldrunner_world_simulation::*;

fn time(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}

fn script_of(
    world: &mut World,
    assembly: &str,
    type_name: &str,
) -> (ObjectHandle, ComponentHandle) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: assembly.into(),
                type_name: type_name.into(),
            },
        )
        .unwrap();
    (object, script)
}

fn frame(world: &mut World) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut work = world.advance(time(1, 50)).unwrap();
    let mut seen = Vec::new();
    loop {
        match work {
            Advance::Callback(callback) => {
                seen.push((callback.script, callback.message));
                work = world
                    .resume(callback.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::Complete(_) => return seen,
            Advance::AssetCallback(_) | Advance::LifecycleComplete => panic!("unexpected work"),
        }
    }
}

fn messages_of(
    seen: &[(ComponentHandle, ScriptMessage)],
    script: ComponentHandle,
) -> Vec<ScriptMessage> {
    seen.iter()
        .filter(|(handle, _)| *handle == script)
        .map(|(_, message)| *message)
        .collect()
}

#[test]
fn a_script_without_fixed_update_gets_update_and_late_update_only() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_messages(
            script,
            ScriptMessages::UPDATE.union(ScriptMessages::LATE_UPDATE),
        )
        .unwrap();
    let seen = frame(&mut world);
    assert_eq!(
        messages_of(&seen, script),
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start,
            ScriptMessage::Update,
            ScriptMessage::LateUpdate,
        ]
    );
}

#[test]
fn a_script_with_no_per_frame_messages_still_starts_once() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_messages(script, ScriptMessages::NONE)
        .unwrap();
    let first = frame(&mut world);
    assert_eq!(
        messages_of(&first, script),
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start
        ]
    );
    assert!(messages_of(&frame(&mut world), script).is_empty());
}

#[test]
fn a_new_script_gets_all_three_messages_whatever_its_siblings_were_given() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, sibling) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_messages(sibling, ScriptMessages::NONE)
        .unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    let seen = frame(&mut world);
    assert_eq!(
        messages_of(&seen, script),
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start,
            ScriptMessage::FixedUpdate,
            ScriptMessage::Update,
            ScriptMessage::LateUpdate,
        ]
    );
    assert!(!messages_of(&seen, sibling).contains(&ScriptMessage::Update));
}

#[test]
fn a_clone_keeps_its_sources_set() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, source) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_messages(source, ScriptMessages::LATE_UPDATE)
        .unwrap();
    let clone = world.instantiate(template, None).unwrap();
    world.take_lifecycle_work(clone.mutation).unwrap();
    let copy = clone.components[&source];
    let seen = frame(&mut world);
    for script in [source, copy] {
        let kinds = messages_of(&seen, script);
        assert!(kinds.contains(&ScriptMessage::LateUpdate), "{script:?}");
        assert!(!kinds.contains(&ScriptMessage::FixedUpdate), "{script:?}");
        assert!(!kinds.contains(&ScriptMessage::Update), "{script:?}");
    }
}

#[test]
fn setting_a_scripts_set_again_replaces_it() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_messages(script, ScriptMessages::UPDATE)
        .unwrap();
    world
        .set_script_messages(script, ScriptMessages::LATE_UPDATE)
        .unwrap();
    let seen = frame(&mut world);
    let kinds = messages_of(&seen, script);
    assert!(kinds.contains(&ScriptMessage::LateUpdate));
    assert!(!kinds.contains(&ScriptMessage::Update));
}

#[test]
fn setting_the_set_of_a_removed_script_is_an_error() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world.remove_component(script).unwrap();
    assert!(matches!(
        world.set_script_messages(script, ScriptMessages::NONE),
        Err(WorldError::StaleComponent(stale)) if stale == script
    ));
}

#[test]
fn setting_the_set_of_a_component_that_is_not_a_script_is_an_error() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let transform = world.object(object).unwrap().transform;
    assert!(matches!(
        world.set_script_messages(transform, ScriptMessages::NONE),
        Err(WorldError::NotScript)
    ));
}

#[test]
fn messages_reject_unknown_bits_and_round_trip_known_ones() {
    assert!(ScriptMessages::from_bits(8).is_none());
    assert!(ScriptMessages::from_bits(0x8000_0000).is_none());
    for bits in 0..8 {
        assert_eq!(ScriptMessages::from_bits(bits).unwrap().bits(), bits);
    }
    assert_eq!(ScriptMessages::ALL.bits(), 7);
    assert_eq!(ScriptMessages::NONE.bits(), 0);
    assert!(ScriptMessages::ALL.contains(ScriptMessages::UPDATE));
    assert!(!ScriptMessages::FIXED_UPDATE.contains(ScriptMessages::UPDATE));
    assert!(ScriptMessages::NONE.contains(ScriptMessages::NONE));
}
