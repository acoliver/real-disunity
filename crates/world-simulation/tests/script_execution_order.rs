//! Scripts receive every message in execution order, lowest first, and in registration order
//! among equal orders.

use worldrunner_world_simulation::*;

fn time(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}

fn script_on(world: &mut World, object: ObjectHandle, type_name: &str) -> ComponentHandle {
    world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: type_name.into(),
            },
        )
        .unwrap()
}

fn script(world: &mut World, type_name: &str) -> ComponentHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    script_on(world, object, type_name)
}

/// The scripts that received `message` in one frame, in delivery order.
fn delivered(world: &mut World, message: ScriptMessage) -> Vec<ComponentHandle> {
    let mut work = world.advance(time(1, 50)).unwrap();
    let mut seen = Vec::new();
    loop {
        match work {
            Advance::Callback(callback) => {
                if callback.message == message {
                    seen.push(callback.script);
                }
                work = world
                    .resume(callback.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::Complete(_) => return seen,
            Advance::AssetCallback(_) | Advance::LifecycleComplete => panic!("unexpected work"),
        }
    }
}

#[test]
fn a_script_with_a_lower_order_receives_each_message_before_one_registered_earlier() {
    for message in [
        ScriptMessage::Start,
        ScriptMessage::FixedUpdate,
        ScriptMessage::Update,
        ScriptMessage::LateUpdate,
    ] {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let setter = script(&mut world, "Setter");
        let clearer = script(&mut world, "Clearer");
        world.set_script_execution_order(clearer, -200).unwrap();
        assert_eq!(
            delivered(&mut world, message),
            vec![clearer, setter],
            "{message:?}"
        );
    }
}

#[test]
fn orders_sort_from_lowest_to_highest_and_equal_orders_keep_registration_order() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let late = script(&mut world, "Late");
    let first = script(&mut world, "First");
    let second = script(&mut world, "Second");
    let early = script(&mut world, "Early");
    world.set_script_execution_order(late, 100).unwrap();
    world.set_script_execution_order(early, -5).unwrap();
    assert_eq!(
        delivered(&mut world, ScriptMessage::Update),
        vec![early, first, second, late]
    );
}

#[test]
fn changing_a_scripts_order_after_frames_have_run_takes_effect_on_the_next_frame() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let first = script(&mut world, "First");
    let second = script(&mut world, "Second");
    assert_eq!(
        delivered(&mut world, ScriptMessage::Update),
        vec![first, second]
    );
    world.set_script_execution_order(second, -1).unwrap();
    assert_eq!(
        delivered(&mut world, ScriptMessage::Update),
        vec![second, first]
    );
}

#[test]
fn a_clone_keeps_its_sources_order() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let source = script_on(&mut world, object, "Source");
    world.set_script_execution_order(source, 30).unwrap();
    let clone = world.instantiate(object, None).unwrap();
    world.take_lifecycle_work(clone.mutation).unwrap();
    let copy = clone.components[&source];
    let other = script(&mut world, "Other");
    assert_eq!(
        delivered(&mut world, ScriptMessage::Update),
        vec![other, source, copy]
    );
}

#[test]
fn setting_the_order_of_a_removed_script_or_a_non_script_is_an_error() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let removed = script(&mut world, "Gone");
    world.remove_component(removed).unwrap();
    assert!(matches!(
        world.set_script_execution_order(removed, 1),
        Err(WorldError::StaleComponent(stale)) if stale == removed
    ));
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let transform = world.object(object).unwrap().transform;
    assert!(matches!(
        world.set_script_execution_order(transform, 1),
        Err(WorldError::NotScript)
    ));
}

/// Two objects with one script each: the setter at order 0 and the clearer at order -200.
fn setter_and_clearer(world: &mut World) -> ([ObjectHandle; 2], [ComponentHandle; 2]) {
    let objects = [(); 2].map(|()| world.create_object(ObjectDesc::default()).unwrap());
    let setter = script_on(world, objects[0], "Setter");
    let clearer = script_on(world, objects[1], "Clearer");
    world.set_script_execution_order(clearer, -200).unwrap();
    (objects, [setter, clearer])
}

#[test]
fn awake_and_on_enable_follow_execution_order() {
    for message in [ScriptMessage::Awake, ScriptMessage::OnEnable] {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let (_, [setter, clearer]) = setter_and_clearer(&mut world);
        assert_eq!(
            delivered(&mut world, message),
            vec![clearer, setter],
            "{message:?}"
        );
    }
}

#[test]
fn awake_and_on_enable_of_a_clone_batch_follow_execution_order() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let setter = script_on(&mut world, object, "Setter");
    let clearer = script_on(&mut world, object, "Clearer");
    world.set_script_execution_order(clearer, -200).unwrap();
    let clone = world.instantiate(object, None).unwrap();
    let (setter, clearer) = (clone.components[&setter], clone.components[&clearer]);
    let batch: Vec<_> = world
        .take_lifecycle_work(clone.mutation)
        .unwrap()
        .into_iter()
        .map(|work| (work.script, work.message))
        .collect();
    assert_eq!(
        batch,
        vec![
            (clearer, ScriptMessage::Awake),
            (setter, ScriptMessage::Awake),
            (clearer, ScriptMessage::OnEnable),
            (setter, ScriptMessage::OnEnable),
        ]
    );
}

#[test]
fn on_disable_and_on_destroy_follow_execution_order() {
    for message in [ScriptMessage::OnDisable, ScriptMessage::OnDestroy] {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let (objects, [setter, clearer]) = setter_and_clearer(&mut world);
        assert_eq!(delivered(&mut world, ScriptMessage::Update).len(), 2);
        for object in objects {
            world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
        }
        assert_eq!(
            delivered(&mut world, message),
            vec![clearer, setter],
            "{message:?}"
        );
    }
}

/// Five scripts of equal order, the second removed, then a sixth registered into its slot.
/// Returns the world and the live scripts in registration order. The sixth takes the freed slot,
/// so its handle sorts before the three registered after the second.
fn five_scripts_with_a_reused_slot() -> (World, Vec<ComponentHandle>) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let scripts: Vec<_> = ["A", "B", "C", "D", "E"]
        .into_iter()
        .map(|name| script_on(&mut world, object, name))
        .collect();
    world.remove_component(scripts[1]).unwrap();
    let replacement = script_on(&mut world, object, "Replacement");
    assert!(
        replacement < scripts[2],
        "the replacement takes the freed slot"
    );
    let in_order = vec![scripts[0], scripts[2], scripts[3], scripts[4], replacement];
    (world, in_order)
}

#[test]
fn a_reused_handle_slot_does_not_move_a_script_ahead_of_earlier_registrations() {
    for message in [
        ScriptMessage::Awake,
        ScriptMessage::OnEnable,
        ScriptMessage::Start,
        ScriptMessage::Update,
    ] {
        let (mut world, in_order) = five_scripts_with_a_reused_slot();
        assert_eq!(delivered(&mut world, message), in_order, "{message:?}");
    }
}
