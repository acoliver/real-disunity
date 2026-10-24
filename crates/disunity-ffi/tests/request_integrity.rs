mod support;
use serde_json::json;
use support::{World, call, copy, submit};
use worldrunner_disunity_ffi::abi::*;

#[test]
fn exact_request_retry_recovers_freed_response_without_creating_again() {
    let world = World::new();
    let bytes = br#"{"version":1,"requestId":"create-1","op":"object.create","name":"once"}"#;
    let (code, response) = submit(world.0, bytes);
    assert_eq!(code, 0);
    let first = copy(response);
    let value: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(value["ok"], true, "requestId must be admitted");
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    let (code, replay) = submit(world.0, bytes);
    assert_eq!(code, 0);
    assert_ne!(response, replay);
    assert_eq!(copy(replay), first);
    assert_eq!(unsafe { disunity_response_free(replay) }.code, 0);
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
    let changed = br#"{ "version":1,"requestId":"create-1","op":"object.create","name":"once"}"#;
    assert_eq!(call(world.0, changed)["ok"], false);
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
    assert_eq!(world.call(json!({"op":"object.create"}))["ok"], true);
    assert_eq!(world.call(json!({"op":"object.create"}))["ok"], true);
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 3);
}

#[test]
fn transaction_commits_once_and_failed_transaction_restores_random_and_world() {
    let world = World::new();
    world.call(json!({"op":"random.seed","seed":123}));
    let expected = world.call(json!({"op":"random.range","min":0,"max":100000}));
    world.call(json!({"op":"random.seed","seed":123}));
    let bytes = br#"{"version":1,"requestId":"tx-1","op":"transaction","operations":[{"op":"object.create","name":"atomic"},{"op":"random.range","min":0,"max":100000}]}"#;
    let first = call(world.0, bytes);
    assert_eq!(first["ok"], true, "transaction must execute");
    assert_eq!(first["data"][1], expected["data"]);
    assert_eq!(call(world.0, bytes), first);
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
    world.call(json!({"op":"random.seed","seed":123}));
    let before = world.snapshot();
    let bad = json!({"version":1,"op":"transaction","operations":[
        {"op":"object.create"}, {"op":"random.range","min":0,"max":100000},
        {"op":"frame.begin","dt":0.1}, {"op":"object.set","object":0,"name":"bad"}
    ]});
    assert_eq!(world.call(bad)["ok"], false);
    assert_eq!(world.snapshot(), before);
    assert_eq!(
        world.call(json!({"op":"random.range","min":0,"max":100000})),
        expected
    );
}

#[test]
fn retained_errors_and_read_results_do_not_reexecute_after_state_changes() {
    let world = World::new();
    let read = br#"{"version":1,"requestId":"snapshot","op":"snapshot"}"#;
    let initial = call(world.0, read);
    assert_eq!(initial["ok"], true);
    world.call(json!({"op":"object.create"}));
    assert_eq!(call(world.0, read), initial);
    assert_ne!(world.snapshot(), initial["data"]);
    let bad = br#"{"version":1,"requestId":"bad","op":"transaction","operations":[{"op":"lifetime.end"}]}"#;
    let error = call(world.0, bad);
    assert_eq!(error["ok"], false);
    assert_eq!(world.call(json!({"op":"lifetime.begin"}))["ok"], true);
    assert_eq!(call(world.0, bad), error);
    assert_eq!(world.call(json!({"op":"lifetime.end"}))["ok"], true);
}

#[test]
fn invalid_envelopes_and_nested_operations_have_no_effect() {
    let world = World::new();
    let before = world.snapshot();
    for bad in [
        json!({"requestId":"unversioned","op":"object.create"}),
        json!({"version":2,"requestId":"version","op":"object.create"}),
        json!({"version":1,"requestId":"","op":"object.create"}),
        json!({"version":1,"requestId":42,"op":"object.create"}),
        json!({"version":1,"requestId":"x".repeat(129),"op":"object.create"}),
        json!({"version":1,"phaseToken":"invented","op":"object.create"}),
        json!({"op":"transaction","operations":[{"op":"object.create"}]}),
        json!({"version":1,"op":"transaction","operations":[]}),
        json!({"version":1,"op":"transaction","operations":[{"op":"object.create"},{"op":"transaction","operations":[]}]}),
        json!({"version":1,"op":"transaction","operations":[{"op":"object.create"},{"op":"object.create","requestId":"nested"}]}),
        json!({"version":1,"op":"transaction","operations":[{"op":"object.create"},{"op":"object.create","version":1}]}),
        json!({"version":1,"op":"transaction","operations":[{"op":"object.create"},{"op":"unknown"}]}),
    ] {
        assert_eq!(world.call(bad.clone())["ok"], false, "{bad}");
        assert_eq!(world.snapshot(), before);
    }
}
