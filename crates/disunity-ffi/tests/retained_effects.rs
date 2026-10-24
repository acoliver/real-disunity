mod support;
use serde_json::json;
use support::World;

#[test]
fn retained_event_drain_replays_real_contacts_without_draining_later_contacts() {
    let world = World::new();
    let mut owners = Vec::new();
    let mut colliders = Vec::new();
    for _ in 0..2 {
        let object = world.call(json!({"op":"object.create"}))["data"]["handle"]
            .as_u64()
            .unwrap();
        assert_eq!(world.call(json!({"op":"component.add","object":object,"kind":"Rigidbody2D","fields":{"constraints":7}}))["ok"], true);
        let collider = world.call(json!({"op":"component.add","object":object,"kind":"BoxCollider2D","fields":{"is_trigger":true}}))["data"]["handle"]
            .as_u64().unwrap();
        owners.push(object);
        colliders.push(collider);
    }
    assert_eq!(
        world.call(json!({"op":"physics.step","dt":0.02}))["ok"],
        true
    );
    let drain = json!({"version":1,"requestId":"drain","op":"events.read"});
    let first = world.call(drain.clone());
    assert_eq!(first["ok"], true);
    let event = &first["data"][0];
    assert_eq!(event["phase"], "enter");
    for key in ["a", "b"] {
        assert!(owners.contains(&event[key].as_u64().unwrap()));
    }
    for key in ["collider_a", "collider_b"] {
        assert!(colliders.contains(&event[key].as_u64().unwrap()));
    }
    assert_eq!(
        world.call(json!({"op":"events.read","clear":false}))["data"],
        json!([])
    );
    assert_eq!(
        world.call(json!({"op":"physics.step","dt":0.02}))["ok"],
        true
    );
    let later = world.call(json!({"op":"events.read","clear":false}));
    assert_eq!(later["data"][0]["phase"], "stay");
    assert_eq!(world.call(drain), first);
    assert_eq!(world.call(json!({"op":"events.read"})), later);
}

#[test]
fn retained_creation_returns_original_retired_handles_and_never_recreates() {
    let world = World::new();
    let request = json!({"version":1,"requestId":"once","op":"transaction","operations":[{"op":"object.create"}]});
    let first = world.call(request.clone());
    assert_eq!(first["ok"], true);
    let object = first["data"][0]["handle"].as_u64().unwrap();
    let transform = first["data"][0]["components"][0]["handle"]
        .as_u64()
        .unwrap();
    assert_eq!(
        world.call(json!({"op":"object.destroy","object":object}))["ok"],
        true
    );
    let replacement = world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    assert_ne!(replacement, object);
    assert_eq!(world.call(request), first);
    assert_eq!(
        world.call(json!({"op":"object.alive","object":object}))["data"],
        false
    );
    assert_eq!(
        world.call(json!({"op":"component.alive","component":transform}))["data"],
        false
    );
    let snapshot = world.snapshot();
    assert_eq!(snapshot["objects"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["objects"][0]["handle"], replacement);
}

#[test]
fn random_replay_does_not_consume_the_next_native_draw() {
    let world = World::new();
    let draw = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    assert_eq!(
        world.call(json!({"op":"random.seed","seed":123}))["ok"],
        true
    );
    let expected_first = world.call(draw.clone());
    let expected_second = world.call(draw.clone());
    world.call(json!({"op":"random.seed","seed":123}));
    let mut retained = draw.clone();
    retained["version"] = json!(1);
    retained["requestId"] = json!("draw");
    assert_eq!(world.call(retained.clone()), expected_first);
    assert_eq!(world.call(retained), expected_first);
    assert_eq!(world.call(draw), expected_second);
}
