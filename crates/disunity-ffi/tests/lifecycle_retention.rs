mod support;

use serde_json::{Value, json};
use support::World;

fn scripts(world: &World) -> Value {
    let response = world.call(json!({"op":"lifecycle.scripts"}));
    assert_eq!(response["ok"], true);
    response["data"].clone()
}

#[test]
fn script_reads_preserve_pending_destruction_rollback_and_exact_identity_replay() {
    let world = World::new();
    let object = world.call(json!({"op":"object.create"}))["data"]["handle"].clone();
    let script = world.call(json!({"op":"component.add","object":object,"kind":"Script"}))["data"]
        ["handle"]
        .clone();
    let expected = scripts(&world);
    let retained = json!({"version":1,"requestId":"scripts","op":"lifecycle.scripts"});
    let response = world.call(retained.clone());
    let before = world.snapshot();
    let failure = world.call(json!({"version":1,"op":"transaction","operations":[
        {"op":"component.set","component":script,"enabled":false},
        {"op":"object.clone","object":object},
        {"op":"object.set","object":0,"active":false}
    ]}));
    assert_eq!(failure["ok"], false);
    assert_eq!(world.snapshot(), before);
    assert_eq!(scripts(&world), expected);
    world.call(json!({"op":"lifetime.begin"}));
    world.call(json!({"op":"component.remove","component":script}));
    world.call(json!({"op":"object.destroy","object":object}));
    assert_eq!(scripts(&world), expected);
    let clone = world.call(json!({"op":"object.clone","object":object}));
    assert_eq!(clone["ok"], true);
    assert_eq!(scripts(&world).as_array().unwrap().len(), 2);
    world.call(json!({"op":"lifetime.end"}));
    assert_eq!(scripts(&world).as_array().unwrap().len(), 1);
    assert_ne!(scripts(&world)[0][0], script);
    assert_eq!(world.call(retained), response);
    let fresh = world
        .call(json!({"version":1,"op":"transaction","operations":[{"op":"lifecycle.scripts"}]}));
    assert_eq!(fresh["data"][0], scripts(&world));
}
