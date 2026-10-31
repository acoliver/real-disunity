//! Which scripts a lifecycle refresh visits, and in what order.

use worldrunner_world_simulation::*;

fn script(world: &mut World, object: ObjectHandle) -> ComponentHandle {
    world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Actor".into(),
            },
        )
        .unwrap()
}

fn inactive_object(world: &mut World) -> ObjectHandle {
    world
        .create_object(ObjectDesc {
            active: false,
            ..Default::default()
        })
        .unwrap()
}

fn run_frame(world: &mut World) {
    let mut next = world.advance(TimeSpan::ZERO).unwrap();
    loop {
        match next {
            Advance::Callback(c) => {
                next = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => return,
            Advance::AssetCallback(_) | Advance::LifecycleComplete => panic!("unexpected work"),
        }
    }
}

fn messages(work: &[LifecycleWork]) -> Vec<(ComponentHandle, ScriptMessage)> {
    work.iter()
        .map(|work| (work.script, work.message))
        .collect()
}

#[test]
fn an_objects_scripts_refresh_in_handle_order_when_a_later_script_reuses_a_lower_slot() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let temporary = world.create_object(ObjectDesc::default()).unwrap();
    script(&mut world, temporary);
    let object = inactive_object(&mut world);
    let first = script(&mut world, object);
    world.destroy(temporary, DestroyWhen::EndOfFrame).unwrap();
    run_frame(&mut world);
    run_frame(&mut world);
    let second = script(&mut world, object);
    assert!(second < first, "the later script took a freed lower slot");
    let mutation = world.set_active(object, true).unwrap();
    let work = world.take_lifecycle_work(mutation).unwrap();
    assert_eq!(
        messages(&work),
        vec![
            (second, ScriptMessage::Awake),
            (first, ScriptMessage::Awake),
            (second, ScriptMessage::OnEnable),
            (first, ScriptMessage::OnEnable),
        ]
    );
}

#[test]
fn a_destroyed_script_awaiting_retirement_is_not_woken_by_activation() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = inactive_object(&mut world);
    let dying = script(&mut world, object);
    let kept = script(&mut world, object);
    run_frame(&mut world);
    world.destroy_component(dying).unwrap();
    let mutation = world.set_active(object, true).unwrap();
    let work = world.take_lifecycle_work(mutation).unwrap();
    assert_eq!(
        messages(&work),
        vec![
            (kept, ScriptMessage::Awake),
            (kept, ScriptMessage::OnEnable)
        ]
    );
}
