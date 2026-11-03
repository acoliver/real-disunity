use super::reference::assert_frame_start_matches;
use super::walk::Walk;
use crate::{
    Advance, CallbackOutcome, ComponentDesc, ComponentHandle, LifecycleWork, MutationId,
    ObjectDesc, ObjectHandle, ParentSpace, ScriptMessage, TimeSpan, World, WorldSettings,
};

#[test]
fn a_long_mixed_history_leaves_the_world_as_the_full_rescan_would() {
    for seed in 0..6 {
        let mut walk = Walk::new(seed);
        for _ in 0..200 {
            if walk.next(3) == 0 {
                walk.advance();
            } else if walk.next(5) == 0 {
                walk.idle_mutation();
            } else {
                walk.mutate();
            }
            assert_frame_start_matches(&walk.world);
            walk.world.lifecycle.assert_index_matches_scripts();
        }
    }
}

fn desc(type_name: &str) -> ComponentDesc {
    ComponentDesc::Script {
        assembly: "Example".into(),
        type_name: type_name.into(),
    }
}

fn scripted(world: &mut World, parent: Option<ObjectHandle>) -> (ObjectHandle, ComponentHandle) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = world.add_component(object, desc("Actor")).unwrap();
    if let Some(parent) = parent {
        world
            .set_parent(object, Some(parent), ParentSpace::KeepLocal)
            .unwrap();
    }
    (object, script)
}

/// Runs one frame and returns the lifecycle messages its scripts received, in order.
fn frame(world: &mut World) -> Vec<(ComponentHandle, ScriptMessage)> {
    frame_with(world, |_| ())
}

/// Runs one frame, calling `inside` once from the first callback, and returns the lifecycle
/// messages its scripts received, in order.
fn frame_with(
    world: &mut World,
    inside: impl FnOnce(&mut World),
) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut inside = Some(inside);
    let mut work = world.advance(TimeSpan::new(1, 50).unwrap()).unwrap();
    let mut seen = Vec::new();
    loop {
        match work {
            Advance::Callback(callback) => {
                if let Some(inside) = inside.take() {
                    inside(world);
                }
                if matches!(
                    callback.message,
                    ScriptMessage::Awake | ScriptMessage::OnEnable | ScriptMessage::OnDisable
                ) {
                    seen.push((callback.script, callback.message));
                }
                work = world
                    .resume(callback.ticket, CallbackOutcome::Returned)
                    .unwrap();
            }
            Advance::Complete(_) => return seen,
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// Runs the lifecycle work of `mutation` the way the host does and returns its messages.
fn run_scope(world: &mut World, mutation: MutationId) -> Vec<(ComponentHandle, ScriptMessage)> {
    let mut work = world.next_lifecycle(mutation).unwrap();
    let mut seen = Vec::new();
    while let Advance::Callback(callback) = work {
        seen.push((callback.script, callback.message));
        work = world
            .resume(callback.ticket, CallbackOutcome::Returned)
            .unwrap();
    }
    seen
}

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

#[test]
fn a_script_added_before_a_lifecycle_scope_wakes_at_the_next_frame() {
    let mut world = world();
    let (_, script) = scripted(&mut world, None);
    assert_eq!(world.flush_cleanup().unwrap(), Advance::LifecycleComplete);
    assert_eq!(
        frame(&mut world),
        vec![
            (script, ScriptMessage::Awake),
            (script, ScriptMessage::OnEnable)
        ]
    );
    assert!(frame(&mut world).is_empty());
}

#[test]
fn publishing_a_construction_reactivates_objects_parented_below_its_objects() {
    let mut world = world();
    let (object, script) = scripted(&mut world, None);
    frame(&mut world);
    let construction = world.begin_construction().unwrap();
    let holder = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    let mutation = world
        .set_parent(object, Some(holder), ParentSpace::KeepLocal)
        .unwrap();
    assert_eq!(
        run_scope(&mut world, mutation),
        vec![(script, ScriptMessage::OnDisable)]
    );
    world.publish_construction(construction).unwrap();
    assert_eq!(frame(&mut world), vec![(script, ScriptMessage::OnEnable)]);
    assert!(frame(&mut world).is_empty());
}

#[test]
fn publishing_a_construction_reactivates_the_whole_subtree_below_it() {
    let mut world = world();
    let (top, top_script) = scripted(&mut world, None);
    let (middle, middle_script) = scripted(&mut world, Some(top));
    let (_, bottom_script) = scripted(&mut world, Some(middle));
    frame(&mut world);
    let construction = world.begin_construction().unwrap();
    let holder = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    let mutation = world
        .set_parent(top, Some(holder), ParentSpace::KeepLocal)
        .unwrap();
    assert_eq!(run_scope(&mut world, mutation).len(), 3);
    world.publish_construction(construction).unwrap();
    assert_eq!(
        frame(&mut world),
        vec![
            (top_script, ScriptMessage::OnEnable),
            (middle_script, ScriptMessage::OnEnable),
            (bottom_script, ScriptMessage::OnEnable)
        ]
    );
}

#[test]
fn an_object_removed_before_the_frame_starts_is_skipped() {
    let mut world = world();
    let (gone, _) = scripted(&mut world, None);
    let (_, kept) = scripted(&mut world, None);
    world.destroy_object(gone).unwrap();
    assert_eq!(
        frame(&mut world),
        vec![
            (kept, ScriptMessage::Awake),
            (kept, ScriptMessage::OnEnable)
        ]
    );
}

#[test]
fn a_scripted_object_in_an_open_construction_waits_for_its_publication() {
    let mut world = world();
    let construction = world.begin_construction().unwrap();
    let object = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    let script = world.add_component(object, desc("Actor")).unwrap();
    assert!(frame(&mut world).is_empty());
    world.publish_construction(construction).unwrap();
    let work = world.take_lifecycle_work(construction.mutation()).unwrap();
    assert_eq!(work.len(), 2);
    assert!(work.iter().all(|work| work.script == script));
    assert!(frame(&mut world).is_empty());
}

#[test]
fn a_frame_after_a_frame_with_no_changes_refreshes_nothing() {
    let mut world = world();
    let (object, script) = scripted(&mut world, None);
    frame(&mut world);
    let mutation = world.set_active(object, false).unwrap();
    world.take_lifecycle_work(mutation).unwrap();
    let mutation = world.set_active(object, true).unwrap();
    assert_eq!(
        world.take_lifecycle_work(mutation).unwrap(),
        vec![LifecycleWork {
            script,
            message: ScriptMessage::OnEnable
        }]
    );
    assert!(frame(&mut world).is_empty());
}

#[test]
fn a_frame_start_with_nothing_to_refresh_queues_no_work() {
    let mut world = world();
    let construction = world.begin_construction().unwrap();
    let object = world
        .create_object_in(construction, ObjectDesc::default())
        .unwrap();
    world.add_component(object, desc("Actor")).unwrap();
    world.scheduler.frame_start = true;
    world.initialize_scripts().unwrap();
    assert!(world.lifecycle.mutations.is_empty());
    scripted(&mut world, None);
    frame(&mut world);
    world.scheduler.frame_start = true;
    world.initialize_scripts().unwrap();
    assert!(world.lifecycle.mutations.is_empty());
}

#[test]
fn a_frame_start_refreshes_objects_in_handle_order() {
    let mut world = world();
    let scripts: Vec<_> = (0..8).map(|_| scripted(&mut world, None).1).collect();
    let awake = scripts.iter().map(|&script| (script, ScriptMessage::Awake));
    let enabled = scripts
        .iter()
        .map(|&script| (script, ScriptMessage::OnEnable));
    let expected: Vec<_> = awake.chain(enabled).collect();
    assert_eq!(frame(&mut world), expected);
}

#[test]
fn a_script_added_inside_a_frame_wakes_before_the_frame_ends() {
    let mut world = world();
    scripted(&mut world, None);
    frame(&mut world);
    let mut added = None;
    let seen = frame_with(&mut world, |world| {
        added = Some(scripted(world, None).1);
    });
    let script = added.unwrap();
    assert_eq!(
        seen,
        vec![
            (script, ScriptMessage::Awake),
            (script, ScriptMessage::OnEnable)
        ]
    );
    assert!(frame(&mut world).is_empty());
}

#[test]
fn a_construction_published_inside_a_frame_reactivates_its_children_at_the_next_frame() {
    let mut world = world();
    let (object, script) = scripted(&mut world, None);
    frame(&mut world);
    let seen = frame_with(&mut world, |world| {
        let construction = world.begin_construction().unwrap();
        let holder = world
            .create_object_in(construction, ObjectDesc::default())
            .unwrap();
        world
            .set_parent(object, Some(holder), ParentSpace::KeepLocal)
            .unwrap();
        world.publish_construction(construction).unwrap();
    });
    assert_eq!(seen, vec![(script, ScriptMessage::OnDisable)]);
    assert_eq!(frame(&mut world), vec![(script, ScriptMessage::OnEnable)]);
}
