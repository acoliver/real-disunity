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
fn a_type_without_fixed_update_gets_update_and_late_update_only() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world
        .set_script_type_messages(
            "Example",
            "Actor",
            ScriptMessages::UPDATE.union(ScriptMessages::LATE_UPDATE),
        )
        .unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
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
fn a_type_with_no_per_frame_messages_still_starts_once() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world
        .set_script_type_messages("Example", "Actor", ScriptMessages::NONE)
        .unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
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
fn a_script_added_before_its_type_is_registered_takes_the_set_on_registration() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_type_messages("Example", "Actor", ScriptMessages::FIXED_UPDATE)
        .unwrap();
    let seen = frame(&mut world);
    assert_eq!(
        messages_of(&seen, script),
        vec![
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::Start,
            ScriptMessage::FixedUpdate,
        ]
    );
}

#[test]
fn an_unregistered_type_gets_all_three_messages() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_type_messages("Example", "Other", ScriptMessages::NONE)
        .unwrap();
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
}

#[test]
fn clones_take_their_types_set() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world
        .set_script_type_messages("Example", "Actor", ScriptMessages::LATE_UPDATE)
        .unwrap();
    let (template, _) = script_of(&mut world, "Example", "Actor");
    let clone = world.instantiate(template, None).unwrap();
    let mutation = clone.mutation;
    world.take_lifecycle_work(mutation).unwrap();
    let seen = frame(&mut world);
    let kinds: Vec<_> = seen.iter().map(|(_, message)| *message).collect();
    assert!(kinds.contains(&ScriptMessage::LateUpdate));
    assert!(!kinds.contains(&ScriptMessage::FixedUpdate));
    assert!(!kinds.contains(&ScriptMessage::Update));
}

#[test]
fn a_clone_made_after_the_type_changed_follows_the_new_set() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (template, _) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_type_messages("Example", "Actor", ScriptMessages::UPDATE)
        .unwrap();
    let clone = world.instantiate(template, None).unwrap();
    world.take_lifecycle_work(clone.mutation).unwrap();
    let seen = frame(&mut world);
    let kinds: Vec<_> = seen.iter().map(|(_, message)| *message).collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|message| **message == ScriptMessage::Update)
            .count(),
        2
    );
    assert!(!kinds.contains(&ScriptMessage::FixedUpdate));
    assert!(!kinds.contains(&ScriptMessage::LateUpdate));
}

#[test]
fn same_named_types_in_different_assemblies_are_separate() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world
        .set_script_type_messages("First", "Actor", ScriptMessages::FIXED_UPDATE)
        .unwrap();
    world
        .set_script_type_messages("Second", "Actor", ScriptMessages::UPDATE)
        .unwrap();
    let (_, first) = script_of(&mut world, "First", "Actor");
    let (_, second) = script_of(&mut world, "Second", "Actor");
    let seen = frame(&mut world);
    assert!(messages_of(&seen, first).contains(&ScriptMessage::FixedUpdate));
    assert!(!messages_of(&seen, first).contains(&ScriptMessage::Update));
    assert!(messages_of(&seen, second).contains(&ScriptMessage::Update));
    assert!(!messages_of(&seen, second).contains(&ScriptMessage::FixedUpdate));
}

#[test]
fn registering_a_type_again_replaces_its_set_for_live_scripts() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (_, script) = script_of(&mut world, "Example", "Actor");
    world
        .set_script_type_messages("Example", "Actor", ScriptMessages::UPDATE)
        .unwrap();
    world
        .set_script_type_messages("Example", "Actor", ScriptMessages::LATE_UPDATE)
        .unwrap();
    let seen = frame(&mut world);
    let kinds = messages_of(&seen, script);
    assert!(kinds.contains(&ScriptMessage::LateUpdate));
    assert!(!kinds.contains(&ScriptMessage::Update));
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

#[test]
fn invalid_type_names_are_rejected() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert!(
        world
            .set_script_type_messages("", "Actor", ScriptMessages::ALL)
            .is_err()
    );
    assert!(
        world
            .set_script_type_messages("Example", "", ScriptMessages::ALL)
            .is_err()
    );
    let long = "x".repeat(1025);
    assert!(
        world
            .set_script_type_messages("Example", &long, ScriptMessages::ALL)
            .is_err()
    );
}
