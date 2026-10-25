mod support;
use serde_json::json;
use support::{World, call, copy, submit};
use worldrunner_disunity_ffi::abi::*;

static SERIALIZE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn descriptor_advertises_only_implemented_integrity_and_actual_limits() {
    let _guard = SERIALIZE.lock().unwrap();
    let world = World::new();
    let descriptor = world.call(json!({"version":1,"op":"describe"}));
    let data = &descriptor["data"];
    for capability in [
        "request_deduplication",
        "atomic_transactions",
        "thread_affinity",
    ] {
        assert!(
            data["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!(capability)),
            "{capability}"
        );
    }
    assert_eq!(data["limits"]["max_retained_requests"], 1024);
    assert_eq!(data["limits"]["max_retained_request_bytes"], 16 << 20);
    assert_eq!(data["limits"]["max_request_id_bytes"], 128);
    assert_eq!(data["limits"]["max_transaction_operations"], 128);
    assert_eq!(
        data["request_identity_policy"],
        "exact_bytes_until_world_destroy"
    );
    assert_eq!(
        data["thread_policy"],
        "owner_mutations_cross_thread_unretained_reads"
    );
    for absent in ["phase_tokens", "recursive_advance_guard", "rational_clock"] {
        assert!(
            !data["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!(absent))
        );
    }
    assert_eq!(call(0, br#"{"version":1,"op":"describe"}"#), descriptor);
    assert_eq!(
        submit(0, br#"{"version":1,"requestId":"global","op":"describe"}"#),
        (1, 0)
    );
}

#[test]
fn retained_identity_count_is_not_freed_with_responses_and_never_evicts() {
    let _guard = SERIALIZE.lock().unwrap();
    let world = World::new();
    let first = br#"{"version":1,"requestId":"0","op":"object.create"}"#;
    let created = call(world.0, first);
    assert_eq!(created["ok"], true);
    for id in 1..1024 {
        assert_eq!(
            world.call(json!({"version":1,"requestId":id.to_string(),"op":"time.get"}))["ok"],
            true
        );
    }
    let before = world.snapshot();
    let full = br#"{"version":1,"requestId":"full","op":"transaction","operations":[{"op":"object.create"},{"op":"frame.begin","dt":0.1}]}"#;
    assert_eq!(submit(world.0, full), (2, 0));
    assert_eq!(world.snapshot(), before);
    assert_eq!(call(world.0, first), created);
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.call(json!({"op":"object.create"}))["ok"], true);
    let stale = world.0;
    drop(world);
    let fresh = World::new();
    assert_eq!(submit(stale, first), (1, 0));
    assert_eq!(call(fresh.0, first)["ok"], true);
}

#[test]
fn semantic_ledger_byte_bound_rejects_before_mutation_and_keeps_replays() {
    let _guard = SERIALIZE.lock().unwrap();
    let world = World::new();
    let object = world.call(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let name = "n".repeat(900_000);
    let first = serde_json::to_vec(
        &json!({"version":1,"requestId":"large-0","op":"object.set","object":object,"name":name}),
    )
    .unwrap();
    let retained = call(world.0, &first);
    let mut rejected = false;
    for id in 1..32 {
        let before = world.snapshot();
        let request = serde_json::to_vec(&json!({"version":1,"requestId":format!("large-{id}"),"op":"object.set","object":object,"name":format!("{id}{name}")})).unwrap();
        let (code, response) = submit(world.0, &request);
        if code == 2 {
            assert_eq!(response, 0);
            assert_eq!(world.snapshot(), before);
            rejected = true;
            break;
        }
        assert_eq!(code, 0);
        assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    }
    assert!(
        rejected,
        "declared byte budget must bind before count budget"
    );
    assert_eq!(call(world.0, &first), retained);
}

#[test]
fn response_handle_budget_rejects_transaction_without_claiming_identity() {
    let _guard = SERIALIZE.lock().unwrap();
    let world = World::new();
    let request = br#"{"version":1,"requestId":"after-free","op":"transaction","operations":[{"op":"object.create"},{"op":"frame.begin","dt":0.1}]}"#;
    let mut held = Vec::new();
    for _ in 0..1024 {
        let (code, response) = submit(world.0, br#"{"op":"time.get"}"#);
        assert_eq!(code, 0);
        held.push(response);
    }
    assert_eq!(submit(world.0, request), (2, 0));
    assert_eq!(
        worldrunner_disunity_ffi::request_value(world.0, json!({"op":"snapshot"})).unwrap()["data"]
            ["objects"],
        json!([])
    );
    assert_eq!(
        unsafe { disunity_response_free(held.pop().unwrap()) }.code,
        0
    );
    let (code, response) = submit(world.0, request);
    assert_eq!(code, 0);
    let first = copy(response);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&first).unwrap()["ok"],
        true
    );
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    let (code, replay) = submit(world.0, request);
    assert_eq!(code, 0);
    assert_eq!(copy(replay), first);
    assert_eq!(unsafe { disunity_response_free(replay) }.code, 0);
    for response in held {
        assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    }
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
}
