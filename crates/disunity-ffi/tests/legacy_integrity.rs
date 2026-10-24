mod support;
use serde_json::json;
use support::World;
use worldrunner_disunity_ffi::{DisunityCommand, abi::*};

#[test]
fn legacy_queue_still_applies_valid_prefix_and_discards_failed_batch_tail() {
    let world = World::new();
    let object = world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let command = DisunityCommand {
        object,
        kind: 0,
        x: 3.0,
        y: 2.0,
        z: 1.0,
    };
    let commands = [
        command,
        DisunityCommand {
            kind: 999,
            ..command
        },
        DisunityCommand { x: 9.0, ..command },
    ];
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world.0, commands.as_ptr(), 3) }.code,
        0
    );
    assert_eq!(
        world.call(json!({"op":"transform.get","object":object}))["data"]["position"],
        json!([0.0, 0.0, 0.0])
    );
    assert_eq!(unsafe { disunity_world_step(world.0, 0.1, 1) }.code, 3);
    assert_eq!(
        world.call(json!({"op":"transform.get","object":object}))["data"]["position"],
        json!([3.0, 2.0, 1.0])
    );
    assert_eq!(world.snapshot()["time"]["frame"], 0);
    let mut message = [0_u8; 128];
    let mut len = 0;
    assert_eq!(
        unsafe { disunity_error_message(world.0, message.as_mut_ptr(), 128, &mut len) }.code,
        0
    );
    assert_eq!(&message[..len as usize], b"unknown command kind");
    assert_eq!(unsafe { disunity_world_step(world.0, 0.0, 2) }.code, 0);
    assert_eq!(
        world.call(json!({"op":"transform.get","object":object}))["data"]["position"],
        json!([3.0, 2.0, 1.0])
    );
}

#[test]
fn failed_transaction_leaves_legacy_error_message_and_command_queue_unchanged() {
    let world = World::new();
    let object = world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let invalid = DisunityCommand {
        object,
        kind: 999,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world.0, &invalid, 1) }.code,
        0
    );
    assert_eq!(unsafe { disunity_world_step(world.0, 0.0, 0) }.code, 3);
    let valid = DisunityCommand {
        kind: 0,
        x: 7.0,
        ..invalid
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world.0, &valid, 1) }.code,
        0
    );
    assert_eq!(
        world.call(json!({"version":1,"op":"transaction","operations":[
            {"op":"object.destroy","object":object}, {"op":"object.create"},
            {"op":"component.get","component":0}
        ]}))["ok"],
        false
    );
    let mut message = [0_u8; 128];
    let mut len = 0;
    assert_eq!(
        unsafe { disunity_error_message(world.0, message.as_mut_ptr(), 128, &mut len) }.code,
        0
    );
    assert_eq!(&message[..len as usize], b"unknown command kind");
    assert_eq!(unsafe { disunity_world_step(world.0, 0.0, 1) }.code, 0);
    assert_eq!(
        world.call(json!({"op":"transform.get","object":object}))["data"]["position"],
        json!([7.0, 0.0, 0.0])
    );
}
