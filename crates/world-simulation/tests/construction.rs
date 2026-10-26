use worldrunner_world_simulation::{
    Advance, CallbackOutcome, ComponentDesc, ComponentHandle, Construction, FieldValue, ObjectDesc,
    ObjectHandle, ParentSpace, ScriptMessage, World, WorldError, WorldSettings,
};

fn script(world: &mut World, object: ObjectHandle, name: &str) -> ComponentHandle {
    world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Game".into(),
                type_name: name.into(),
            },
        )
        .unwrap()
}

fn named(name: &str, active: bool) -> ObjectDesc {
    ObjectDesc {
        name: name.into(),
        active,
        ..ObjectDesc::default()
    }
}

/// A scene of three objects: `Root` with a child `Child`, and an inactive `Hidden`.
/// Each has one script; `Root`'s script references `Child`'s.
struct Scene {
    construction: Construction,
    root: ObjectHandle,
    scripts: [ComponentHandle; 3],
}

fn construct(world: &mut World) -> Scene {
    let construction = world.begin_construction().unwrap();
    let root = world
        .create_object_in(construction, named("Root", true))
        .unwrap();
    let child = world
        .create_object_in(construction, named("Child", true))
        .unwrap();
    let hidden = world
        .create_object_in(construction, named("Hidden", false))
        .unwrap();
    world
        .set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    let scripts = [
        script(world, root, "Manager"),
        script(world, child, "Player"),
        script(world, hidden, "Pool"),
    ];
    world
        .set_script_fields(
            scripts[0],
            vec![("player".into(), FieldValue::Component(scripts[1]))],
        )
        .unwrap();
    Scene {
        construction,
        root,
        scripts,
    }
}

#[test]
fn constructed_objects_stay_out_of_the_world_until_published() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let existing = world.create_object(named("Existing", true)).unwrap();
    let scene = construct(&mut world);
    let names: Vec<_> = world
        .snapshot()
        .objects
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert_eq!(names, ["Existing"]);
    assert_eq!(world.parent(existing).unwrap(), None);
    assert!(!world.script_ready(scene.scripts[0]).unwrap());
    assert_eq!(
        world.lifecycle_work(scene.construction.mutation()),
        Err(WorldError::WrongMutation),
        "no lifecycle work exists before publishing"
    );
}

#[test]
fn publishing_queues_awake_and_enable_for_every_active_constructed_script() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let scene = construct(&mut world);
    world.publish_construction(scene.construction).unwrap();
    assert_eq!(world.snapshot().objects.len(), 3);
    let work: Vec<_> = world
        .lifecycle_work(scene.construction.mutation())
        .unwrap()
        .iter()
        .map(|work| (work.script, work.message))
        .collect();
    let [manager, player, _hidden] = scene.scripts;
    assert_eq!(
        work,
        [
            (manager, ScriptMessage::Awake),
            (player, ScriptMessage::Awake),
            (manager, ScriptMessage::OnEnable),
            (player, ScriptMessage::OnEnable),
        ],
        "every Awake precedes every OnEnable; the inactive object's script waits"
    );
    assert_eq!(
        world.script_fields(manager).unwrap(),
        [("player".to_owned(), FieldValue::Component(player))],
        "fields bound during construction survive publishing"
    );
    assert_eq!(world.object(scene.root).unwrap().name, "Root");
}

#[test]
fn discarding_removes_everything_the_construction_created() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.create_object(named("Existing", true)).unwrap();
    let before = world.storage_counts();
    let scene = construct(&mut world);
    world.discard_construction(scene.construction).unwrap();
    assert_eq!(world.storage_counts(), before);
    assert_eq!(world.snapshot().objects.len(), 1);
    assert!(world.object(scene.root).is_err());
    assert!(world.script_fields(scene.scripts[0]).is_err());
    let again = world.discard_construction(scene.construction);
    assert_eq!(again, Err(WorldError::WrongMutation));
}

#[test]
fn a_construction_closes_when_published_and_belongs_to_one_world() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let scene = construct(&mut world);
    world.publish_construction(scene.construction).unwrap();
    let closed = Err(WorldError::WrongMutation);
    assert_eq!(world.publish_construction(scene.construction), closed);
    assert_eq!(world.discard_construction(scene.construction), closed);
    assert_eq!(
        world
            .create_object_in(scene.construction, ObjectDesc::default())
            .err(),
        Some(WorldError::WrongMutation)
    );
}

#[test]
fn a_construction_from_another_world_is_rejected_even_with_the_same_sequence() {
    let mut first = World::new(WorldSettings::default()).unwrap();
    let mut second = World::new(WorldSettings::default()).unwrap();
    let foreign = first.begin_construction().unwrap();
    let local = second.begin_construction().unwrap();
    let rejected = Some(WorldError::WrongMutation);
    assert_eq!(
        second
            .create_object_in(foreign, ObjectDesc::default())
            .err(),
        rejected
    );
    assert_eq!(second.publish_construction(foreign).err(), rejected);
    assert_eq!(second.discard_construction(foreign).err(), rejected);
    second
        .create_object_in(local, ObjectDesc::default())
        .unwrap();
    second.publish_construction(local).unwrap();
    assert_eq!(second.snapshot().objects.len(), 1);
}

/// Runs `advance` to completion, recording each script callback.
fn drain(world: &mut World, mut advance: Advance) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut calls = Vec::new();
    loop {
        match advance {
            Advance::Callback(work) => {
                calls.push((work.script, work.message));
                advance = world
                    .resume(work.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::LifecycleComplete => return calls,
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn activation_publishes_and_runs_awake_for_scenes_and_clones() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let scene = construct(&mut world);
    let advance = world.activate_construction(scene.construction).unwrap();
    let [manager, player, _hidden] = scene.scripts;
    assert_eq!(
        drain(&mut world, advance),
        [
            (manager, ScriptMessage::Awake),
            (player, ScriptMessage::Awake),
            (manager, ScriptMessage::OnEnable),
            (player, ScriptMessage::OnEnable),
        ]
    );
    assert!(world.script_ready(manager).unwrap());
    let clone = world.instantiate_unpublished(scene.root, None).unwrap();
    let cloned = clone.components[&manager];
    assert!(!world.script_ready(cloned).unwrap());
    let advance = world.activate_construction(clone.construction()).unwrap();
    let calls = drain(&mut world, advance);
    assert_eq!(calls[0], (cloned, ScriptMessage::Awake));
    assert_eq!(calls.len(), 4, "the clone's two scripts wake and enable");
    assert!(world.script_ready(cloned).unwrap());
    assert_eq!(
        world.activate_construction(clone.construction()).err(),
        Some(WorldError::WrongMutation)
    );
}

#[test]
fn waking_runs_new_scripts_without_changing_activity() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let active = world.create_object(named("Active", true)).unwrap();
    let inactive = world.create_object(named("Inactive", false)).unwrap();
    let added = script(&mut world, active, "Added");
    let waiting = script(&mut world, inactive, "Waiting");
    let mutation = world.wake(active).unwrap();
    let work: Vec<_> = world
        .lifecycle_work(mutation)
        .unwrap()
        .iter()
        .map(|work| (work.script, work.message))
        .collect();
    assert_eq!(
        work,
        [
            (added, ScriptMessage::Awake),
            (added, ScriptMessage::OnEnable)
        ]
    );
    let mutation = world.wake(inactive).unwrap();
    assert!(world.lifecycle_work(mutation).unwrap().is_empty());
    assert!(
        !world.object(inactive).unwrap().active,
        "waking never activates"
    );
    assert!(!world.script_ready(waiting).unwrap());
    let construction = world.begin_construction().unwrap();
    let pending = world
        .create_object_in(construction, named("Pending", true))
        .unwrap();
    script(&mut world, pending, "Pending");
    let mutation = world.wake(pending).unwrap();
    assert!(
        world.lifecycle_work(mutation).unwrap().is_empty(),
        "constructing objects wait for their construction"
    );
}
