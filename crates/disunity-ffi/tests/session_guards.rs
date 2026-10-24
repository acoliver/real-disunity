mod support;
use serde_json::json;
use support::{World, call};
use worldrunner_disunity_ffi::{DisunityCommand, abi::*, request_value, with_world};

#[test]
fn wrong_thread_mutation_and_advance_reject_without_changing_world_or_queue() {
    let world = World::new();
    let object = world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let before = world.snapshot();
    let handle = world.0;
    std::thread::spawn(move || {
        let mut created = 0;
        assert_eq!(
            unsafe { disunity_gameobject_create(handle, &mut created) }.code,
            4,
            "wrong-thread object creation must be rejected"
        );
        assert_eq!(created, 0);
        let command = DisunityCommand {
            object,
            kind: 0,
            x: 9.0,
            y: 0.0,
            z: 0.0,
        };
        assert_eq!(
            unsafe { disunity_command_buffer_submit(handle, &command, 1) }.code,
            4
        );
        assert_eq!(
            unsafe { disunity_transform_set_position(handle, object, 9.0, 0.0, 0.0) }.code,
            4
        );
        assert_eq!(
            unsafe { disunity_transform_set_rotation_y(handle, object, 1.0) }.code,
            4
        );
        assert_eq!(
            unsafe { disunity_gameobject_destroy(handle, object) }.code,
            4
        );
        assert_eq!(unsafe { disunity_world_step(handle, 0.1, 1) }.code, 4);
        assert_eq!(unsafe { disunity_world_destroy(handle) }.code, 4);
        assert_eq!(call(handle, br#"{"op":"object.create"}"#)["ok"], false);
        assert_eq!(
            call(handle, br#"{"op":"frame.begin","dt":0.1}"#)["ok"],
            false
        );
        assert_eq!(
            request_value(handle, json!({"op":"object.create"})).unwrap()["ok"],
            false
        );
        assert_eq!(call(handle, br#"{"op":"snapshot"}"#)["data"], before);
    })
    .join()
    .unwrap();
    assert!(
        with_world(handle, |w| w.pending_commands.is_empty()
            && w.last_error.is_none())
        .unwrap()
    );
    assert_eq!(unsafe { disunity_world_step(handle, 0.0, 0) }.code, 0);
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
}

#[test]
fn opaque_world_object_and_component_generations_never_cross_owners() {
    let world = World::new();
    let foreign = World::new();
    let object = foreign.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let component =
        foreign.call(json!({"op":"component.list","object":object}))["data"][0]["handle"]
            .as_u64()
            .unwrap();
    let before = world.snapshot();
    let result = world.call(json!({"version":1,"op":"transaction","operations":[
        {"op":"object.create"}, {"op":"component.set","component":component,"enabled":false}
    ]}));
    assert_eq!(result["ok"], false);
    assert_eq!(world.snapshot(), before);
    assert_eq!(
        world.call(json!({"op":"object.get","object":object}))["ok"],
        false
    );
    let stale = foreign.0;
    drop(foreign);
    let replacement = World::new();
    assert_ne!(stale, replacement.0);
    assert_eq!(support::submit(stale, br#"{"op":"snapshot"}"#), (1, 0));
    assert_eq!(
        replacement.call(json!({"op":"object.alive","object":object}))["data"],
        false
    );
}
