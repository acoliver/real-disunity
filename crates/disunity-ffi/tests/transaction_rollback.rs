mod support;
use serde_json::{Value, json};
use support::World;
use worldrunner_disunity_ffi::{DisunityCommand, abi::*};

fn object(world: &World) -> u64 {
    world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap()
}
fn add(world: &World, object: u64, kind: &str, fields: Value) -> u64 {
    let value =
        world.call(json!({"op":"component.add","object":object,"kind":kind,"fields":fields}));
    assert_eq!(value["ok"], true);
    value["data"]["handle"].as_u64().unwrap()
}
fn events(world: &World) -> Value {
    world.call(json!({"op":"events.read","clear":false}))["data"].clone()
}
fn random(world: &World) -> Value {
    world.call(json!({"op":"random.range","min":0,"max":1000000,"integer":true}))
}

#[test]
fn rollback_preserves_solver_contacts_event_drain_lifetime_and_legacy_pending_work() {
    let world = World::new();
    let ball = object(&world);
    let wall = object(&world);
    let body = add(&world, ball, "Rigidbody2D", json!({"constraints":7}));
    add(&world, ball, "CircleCollider2D", json!({"is_trigger":true}));
    add(&world, wall, "BoxCollider2D", json!({}));
    assert_eq!(
        world.call(json!({"op":"physics.step","dt":0.02}))["ok"],
        true
    );
    let contacts = events(&world);
    assert_eq!(contacts[0]["phase"], "enter");
    let command = DisunityCommand {
        object: ball,
        kind: 0,
        x: 7.0,
        y: 0.0,
        z: 0.0,
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world.0, &command, 1) }.code,
        0
    );
    world.call(json!({"op":"random.seed","seed":123}));
    let expected = random(&world);
    world.call(json!({"op":"random.seed","seed":123}));
    let before = world.snapshot();
    let failed = world.call(
        json!({"version":1,"requestId":"rollback","op":"transaction","operations":[
            {"op":"events.read"}, {"op":"physics.step","dt":0.03},
            {"op":"component.set","component":body,"fields":{"constraints":0,"velocity":[10,0]}},
            {"op":"physics.layers","layer_a":0,"layer_b":0,"ignore":true},
            {"op":"lifetime.begin"}, {"op":"object.destroy","object":ball},
            {"op":"object.create"}, {"op":"random.range","min":0,"max":1000000},
            {"op":"object.set","object":0,"name":"fail"}
        ]}),
    );
    assert_eq!(failed["ok"], false);
    assert_eq!(world.snapshot(), before);
    assert_eq!(events(&world), contacts);
    assert_eq!(world.call(json!({"op":"events.read"}))["data"], contacts);
    assert_eq!(random(&world), expected);
    assert_eq!(world.call(json!({"op":"lifetime.end"}))["ok"], false);
    assert_eq!(
        world.call(json!({"op":"physics.step","dt":0.02}))["ok"],
        true
    );
    assert_eq!(events(&world)[0]["phase"], "stay");
    assert_eq!(unsafe { disunity_world_step(world.0, 0.0, 1) }.code, 0);
    assert_eq!(
        world.call(json!({"op":"transform.get","object":ball}))["data"]["position"],
        json!([7.0, 0.0, 0.0])
    );
}

#[test]
fn oversized_aggregate_response_rolls_back_and_retains_error_independently_of_buffers() {
    let world = World::new();
    let object = object(&world);
    world.call(json!({"op":"object.set","object":object,"name":"n".repeat(600_000)}));
    world.call(json!({"op":"random.seed","seed":123}));
    let expected = random(&world);
    world.call(json!({"op":"random.seed","seed":123}));
    let before = world.snapshot();
    let mut operations = vec![
        json!({"op":"lifetime.begin"}),
        json!({"op":"object.destroy","object":object}),
        json!({"op":"object.set","object":object,"active":false}),
        json!({"op":"random.range","min":0,"max":1000000}),
        json!({"op":"object.create"}),
    ];
    operations.extend((0..30).map(|_| json!({"op":"object.get","object":object})));
    let request =
        json!({"version":1,"requestId":"oversized","op":"transaction","operations":operations});
    let error = world.call(request.clone());
    assert_eq!(error["ok"], false);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("response budget exceeded")
    );
    assert_eq!(world.snapshot(), before);
    assert_eq!(random(&world), expected);
    assert_eq!(world.call(json!({"op":"lifetime.end"}))["ok"], false);
    world.call(json!({"op":"object.set","object":object,"name":"short"}));
    assert_eq!(world.call(request.clone()), error);
    let mut different_id = request;
    different_id["requestId"] = json!("now-fits");
    assert_eq!(world.call(different_id)["ok"], true);
    assert_eq!(world.call(json!({"op":"lifetime.end"}))["ok"], true);
    assert_eq!(
        world.call(json!({"op":"object.alive","object":object}))["data"],
        false
    );
}

#[test]
fn operation_bounds_and_duplicate_singletons_are_atomic() {
    let world = World::new();
    let object = object(&world);
    let before = world.snapshot();
    let too_many = vec![json!({"op":"object.create"}); 129];
    assert_eq!(
        world.call(json!({"version":1,"op":"transaction","operations":too_many}))["ok"],
        false
    );
    assert_eq!(world.snapshot(), before);
    let result = world.call(json!({"version":1,"op":"transaction","operations":[
        {"op":"component.add","object":object,"kind":"Rigidbody2D"},
        {"op":"component.add","object":object,"kind":"Rigidbody2D"}
    ]}));
    assert_eq!(result["ok"], false);
    assert_eq!(world.snapshot(), before);
    let max = vec![json!({"op":"time.get"}); 128];
    assert_eq!(
        world.call(json!({"version":1,"op":"transaction","operations":max}))["data"]
            .as_array()
            .unwrap()
            .len(),
        128
    );
}
