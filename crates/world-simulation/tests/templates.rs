//! A construction that is never activated holds prefab templates: its objects take part in
//! nothing, and instantiating one publishes an independent clone.
use worldrunner_world_simulation::{
    Advance, CallbackOutcome, ComponentDesc, ComponentHandle, Construction, FieldValue, ObjectDesc,
    ObjectHandle, ParentSpace, ScriptMessage, TimeSpan, World, WorldSettings,
};

fn script(world: &mut World, object: ObjectHandle, name: &str) -> ComponentHandle {
    let desc = ComponentDesc::Script {
        assembly: "Game".into(),
        type_name: name.into(),
    };
    world.add_component(object, desc).unwrap()
}

fn named(name: &str) -> ObjectDesc {
    ObjectDesc {
        name: name.into(),
        ..ObjectDesc::default()
    }
}

/// Breakout's brick prefab in miniature: `Brick` with a child `Sparkle`, whose scripts reference
/// each other.
struct Prefab {
    templates: Construction,
    root: ObjectHandle,
    brick: ComponentHandle,
    sparkle: ComponentHandle,
}

fn prefab(world: &mut World) -> Prefab {
    let templates = world.begin_construction().unwrap();
    let root = world.create_object_in(templates, named("Brick")).unwrap();
    let child = world.create_object_in(templates, named("Sparkle")).unwrap();
    world
        .set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    let brick = script(world, root, "Brick");
    let sparkle = script(world, child, "Sparkle");
    let field = vec![("sparkle".into(), FieldValue::Component(sparkle))];
    world.set_script_fields(brick, field).unwrap();
    Prefab {
        templates,
        root,
        brick,
        sparkle,
    }
}

/// Runs `advance` to completion, returning every script that received a callback.
fn drain(world: &mut World, mut advance: Advance) -> Vec<ComponentHandle> {
    let mut called = Vec::new();
    loop {
        match advance {
            Advance::Callback(work) => {
                called.push(work.script);
                advance = world
                    .resume(work.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::Complete(_) | Advance::LifecycleComplete => return called,
            other @ Advance::AssetCallback(_) => panic!("unexpected {other:?}"),
        }
    }
}

fn frame(world: &mut World) -> Vec<ComponentHandle> {
    let advance = world
        .advance(TimeSpan::from_nanos(16_666_667).unwrap())
        .unwrap();
    drain(world, advance)
}

#[test]
fn a_template_takes_part_in_nothing_until_cloned() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let prefab = prefab(&mut world);
    let spawner = world.create_object(named("Spawner")).unwrap();
    let spawn = script(&mut world, spawner, "Spawner");
    let reference = vec![("brick".into(), FieldValue::Object(prefab.root))];
    world.set_script_fields(spawn, reference).unwrap();
    for _ in 0..3 {
        let called = frame(&mut world);
        assert!(!called.contains(&prefab.brick) && !called.contains(&prefab.sparkle));
    }
    let names: Vec<_> = world
        .snapshot()
        .objects
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert_eq!(names, ["Spawner"], "templates are not in the world");
    assert!(!world.active_in_hierarchy(prefab.root).unwrap());
    assert!(!world.script_ready(prefab.brick).unwrap());
}

#[test]
fn each_instantiation_publishes_an_independent_clone_with_remapped_fields() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let prefab = prefab(&mut world);
    let first = world.instantiate(prefab.root, None).unwrap();
    let second = world.instantiate(prefab.root, None).unwrap();
    for clone in [&first, &second] {
        let brick = clone.components[&prefab.brick];
        let sparkle = clone.components[&prefab.sparkle];
        let work: Vec<_> = world
            .lifecycle_work(clone.mutation)
            .unwrap()
            .iter()
            .map(|work| (work.script, work.message))
            .collect();
        assert_eq!(
            work,
            [
                (brick, ScriptMessage::Awake),
                (sparkle, ScriptMessage::Awake),
                (brick, ScriptMessage::OnEnable),
                (sparkle, ScriptMessage::OnEnable),
            ],
            "only the clone wakes"
        );
        assert_eq!(
            world.script_fields(brick).unwrap(),
            [("sparkle".to_owned(), FieldValue::Component(sparkle))],
            "references inside the prefab point into the clone"
        );
        assert!(world.active_in_hierarchy(clone.root).unwrap());
    }
    assert_ne!(first.root, second.root);
    assert_eq!(world.object(first.root).unwrap().name, "Brick(Clone)");
    let child = world.children(first.root).unwrap()[0];
    assert_eq!(
        world.object(child).unwrap().name,
        "Sparkle",
        "only the root is renamed"
    );
    assert_eq!(
        world.snapshot().objects.len(),
        4,
        "two clones of two objects"
    );
    assert_eq!(
        world.script_fields(prefab.brick).unwrap(),
        [("sparkle".to_owned(), FieldValue::Component(prefab.sparkle))],
        "the template keeps its own fields"
    );
    assert!(!world.active_in_hierarchy(prefab.root).unwrap());
}

#[test]
fn discarding_templates_keeps_their_clones() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let prefab = prefab(&mut world);
    let clone = world.instantiate(prefab.root, None).unwrap();
    let advance = world.next_lifecycle(clone.mutation).unwrap();
    let woken = drain(&mut world, advance);
    assert_eq!(woken.len(), 4);
    world.discard_construction(prefab.templates).unwrap();
    assert!(world.object(prefab.root).is_err());
    assert!(world.object(clone.root).is_ok());
    assert_eq!(world.snapshot().objects.len(), 2);
    assert!(
        world.instantiate(prefab.root, None).is_err(),
        "a discarded template is gone"
    );
}
