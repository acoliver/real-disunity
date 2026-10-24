mod transport_support;
use serde_json::{Value, json};
use transport_support::*;
use worldrunner_disunity_ffi::response::*;

fn sized_transaction(world: &World, size: usize) -> Value {
    let large =
        world.call(json!({"op":"object.create","name":"x".repeat(600_000)}))["data"]["handle"]
            .as_u64()
            .unwrap();
    let tail = world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let repeats = size / 600_000;
    let mut operations = vec![json!({"op":"object.get","object":large}); repeats];
    operations.push(json!({"op":"object.set","object":tail,"name":""}));
    let mut request = json!({"version":1,"op":"transaction","operations":operations});
    let base = serde_json::to_vec(&world.call(request.clone()))
        .unwrap()
        .len();
    request["operations"][repeats]["name"] = json!("p".repeat(size - base));
    request
}

#[test]
fn exact_eight_mib_reply_is_admitted_and_plus_one_rolls_back_and_replays_error() {
    let world = World::new();
    assert_eq!(world.select()["ok"], true);
    let request = sized_transaction(&world, REPLY);
    let bytes = serde_json::to_vec(&request).unwrap();
    let handle = request_bytes(world.0, &bytes).unwrap();
    assert_eq!(response_bytes(handle).unwrap().len(), REPLY);
    response_free(handle).unwrap();
    let before = world.call(json!({"op":"snapshot"}));
    let mut overflow = request;
    overflow["requestId"] = json!("overflow");
    let tail = overflow["operations"][13]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    overflow["operations"][13]["name"] = json!(tail + "x");
    let error = world.call(overflow.clone());
    assert_eq!(error["ok"], false);
    assert_eq!(
        error["error"]["message"],
        "response budget exceeded; operation rolled back"
    );
    assert_eq!(world.call(json!({"op":"snapshot"})), before);
    assert_eq!(world.call(overflow), error);
    assert_eq!(
        world.call(json!({"version":1,"op":"transport.get"}))["data"]["limits"]["max_outstanding_bytes"],
        OUTSTANDING
    );
}

#[test]
fn exact_request_cap_is_accepted_and_plus_one_precedes_parse_and_mutation() {
    let world = World::new();
    assert_eq!(world.select()["ok"], true);
    let mut request = br#"{"version":1,"op":"object.create"}"#.to_vec();
    request.resize(MAX_REQUEST_BYTES, b' ');
    let handle = request_bytes(world.0, &request).unwrap();
    response_free(handle).unwrap();
    request.push(b' ');
    assert_eq!(request_bytes(world.0, &request), Err(2));
    assert_eq!(request_bytes(world.0, b"{"), Err(4));
    assert_eq!(
        world.call(json!({"op":"snapshot"}))["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn owner_quota_is_exact_and_replay_needs_fresh_admission_without_reexecution() {
    let world = World::new();
    assert_eq!(world.select()["ok"], true);
    let mut request = sized_transaction(&world, REPLY);
    request["requestId"] = json!("retained");
    let bytes = serde_json::to_vec(&request).unwrap();
    let first = request_bytes(world.0, &bytes).unwrap();
    let second = request_bytes(world.0, &bytes).unwrap();
    assert_eq!(
        response_bytes(first).unwrap(),
        response_bytes(second).unwrap()
    );
    assert_eq!(request_bytes(world.0, &bytes), Err(2));
    assert_eq!(request_bytes(world.0, br#"{"op":"object.create"}"#), Err(2));
    response_free(second).unwrap();
    let retry = request_bytes(world.0, &bytes).unwrap();
    assert_eq!(response_bytes(retry).unwrap().len(), REPLY);
    response_free(first).unwrap();
    response_free(retry).unwrap();
    assert_eq!(
        world.call(json!({"op":"snapshot"}))["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn rejected_owner_quota_rolls_back_transaction_and_identity_then_retry_executes_once() {
    let world = World::new();
    assert_eq!(world.select()["ok"], true);
    let large = sized_transaction(&world, REPLY);
    let encoded = serde_json::to_vec(&large).unwrap();
    let first = request_bytes(world.0, &encoded).unwrap();
    let second = request_bytes(world.0, &encoded).unwrap();
    let before =
        worldrunner_disunity_ffi::request_value(world.0, json!({"op":"snapshot"})).unwrap();
    let request = json!({"version":1,"requestId":"quota-retry","op":"transaction","operations":[
        {"op":"object.create"}, {"op":"random.range","min":0,"max":1000000,"integer":true},
        {"op":"input.set","keys":["Space"]}, {"op":"time.set","time_scale":0.5},
        {"op":"transform.set","object":before["data"]["objects"][0]["handle"],"position":[1,2,3]}
    ]});
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(request_bytes(world.0, &bytes), Err(2));
    assert_eq!(
        worldrunner_disunity_ffi::request_value(world.0, json!({"op":"snapshot"})).unwrap(),
        before
    );
    response_free(first).unwrap();
    response_free(second).unwrap();
    let accepted = world.call(request.clone());
    assert_eq!(accepted["ok"], true);
    assert_eq!(world.call(request), accepted);
    assert_eq!(
        world.call(json!({"op":"snapshot"}))["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn retired_owner_bytes_survive_destroy_reuse_without_charging_new_generation_or_legacy() {
    let mut retained = Vec::new();
    let mut old_world = None;
    for _ in 0..5 {
        let world = World::new();
        if let Some(old) = old_world {
            assert_eq!(world.0 as u32, old as u32);
            assert_ne!(world.0, old);
        }
        assert_eq!(world.select()["ok"], true);
        let request = serde_json::to_vec(&sized_transaction(&world, REPLY)).unwrap();
        let first = request_bytes(world.0, &request).unwrap();
        let second = request_bytes(world.0, &request).unwrap();
        assert_eq!(request_bytes(world.0, b"{\"op\":\"time.get\"}"), Err(2));
        old_world = Some(world.0);
        retained.extend([first, second]);
    }
    let legacy = World::new();
    assert_eq!(legacy.call(json!({"op":"object.create"}))["ok"], true);
    let target = World::new();
    assert_eq!(target.select()["ok"], true);
    let request = serde_json::to_vec(&sized_transaction(&target, REPLY)).unwrap();
    let first = request_bytes(target.0, &request).unwrap();
    let second = request_bytes(target.0, &request).unwrap();
    let old = retained.pop().unwrap();
    std::thread::spawn(move || {
        assert_eq!(response_bytes(old).unwrap().len(), REPLY);
        response_free(old).unwrap();
        assert_eq!(response_free(old), Err(1));
    })
    .join()
    .unwrap();
    assert_eq!(request_bytes(target.0, b"{\"op\":\"time.get\"}"), Err(2));
    for response in retained.into_iter().chain([first, second]) {
        response_free(response).unwrap();
    }
}

#[test]
fn process_handle_limit_is_shared_and_rejected_selection_remains_retryable() {
    let source = World::new();
    assert_eq!(source.select()["ok"], true);
    let fresh = World::new();
    let mut handles = Vec::new();
    for _ in 0..MAX_RESPONSES {
        handles.push(request_bytes(source.0, b"{\"op\":\"time.get\"}").unwrap());
    }
    let select = serde_json::to_vec(
        &json!({"version":1,"op":"transport.select","transport_profile":PROFILE}),
    )
    .unwrap();
    assert_eq!(request_bytes(fresh.0, &select), Err(2));
    response_free(handles.pop().unwrap()).unwrap();
    let selected = request_bytes(fresh.0, &select).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&response_bytes(selected).unwrap()).unwrap()["ok"],
        true
    );
    response_free(selected).unwrap();
    for handle in handles {
        response_free(handle).unwrap();
    }
    assert_eq!(fresh.select()["ok"], false);
}

#[test]
fn selection_reply_uses_target_bucket_while_legacy_pool_is_full() {
    let target = World::new();
    let legacy = World::new();
    let request = serde_json::to_vec(&sized_transaction(&legacy, MAX_RESPONSE_BYTES)).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| request_bytes(legacy.0, &request).unwrap())
        .collect();
    assert_eq!(request_bytes(legacy.0, b"{\"op\":\"time.get\"}"), Err(2));
    assert_eq!(request_bytes(0, b"{\"op\":\"describe\"}"), Err(2));
    assert_eq!(request_bytes(u64::MAX, b"{\"op\":\"time.get\"}"), Err(2));
    let select = serde_json::to_vec(
        &json!({"version":1,"op":"transport.select","transport_profile":PROFILE}),
    )
    .unwrap();
    let selected = request_bytes(target.0, &select).unwrap();
    let exact = serde_json::to_vec(&sized_transaction(&target, REPLY)).unwrap();
    let first = request_bytes(target.0, &exact).unwrap();
    assert_eq!(request_bytes(target.0, &exact), Err(2));
    response_free(selected).unwrap();
    let second = request_bytes(target.0, &exact).unwrap();
    for handle in handles.into_iter().chain([first, second]) {
        response_free(handle).unwrap();
    }
}

#[test]
fn target_read_serialization_stops_at_eight_mib() {
    let target = World::new();
    assert_eq!(target.select()["ok"], true);
    for _ in 0..13 {
        target.call(json!({"op":"object.create","name":"x".repeat(600_000)}));
    }
    let tail = target.call(json!({"op":"object.create","name":""}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let baseline = serde_json::to_vec(&target.call(json!({"op":"snapshot"})))
        .unwrap()
        .len();
    let padding = "x".repeat(REPLY - baseline);
    target.call(json!({"op":"object.set","object":tail,"name":padding}));
    let exact = request_bytes(target.0, b"{\"op\":\"snapshot\"}").unwrap();
    assert_eq!(response_bytes(exact).unwrap().len(), REPLY);
    response_free(exact).unwrap();
    target.call(json!({"op":"object.set","object":tail,"name":padding + "x"}));
    assert_eq!(
        target.call(json!({"op":"snapshot"}))["error"]["message"],
        "response budget exceeded; operation rolled back"
    );
}
