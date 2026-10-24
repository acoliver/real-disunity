mod support;
use serde_json::json;
use support::{World, call, copy, submit};
use worldrunner_disunity_ffi::abi::*;

#[test]
fn semantic_validation_errors_claim_identity_and_replay_exact_bytes_after_free() {
    let world = World::new();
    let invalid =
        br#"{"version":1,"requestId":"invalid-shape","op":"object.create","invented":true}"#;
    let (code, response) = submit(world.0, invalid);
    assert_eq!(code, 0);
    let bytes = copy(response);
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["ok"],
        false
    );
    let corrected = br#"{"version":1,"requestId":"invalid-shape","op":"object.create"}"#;
    let conflict = call(world.0, corrected);
    assert_eq!(conflict["ok"], false);
    assert!(
        conflict["error"]["message"]
            .as_str()
            .unwrap()
            .contains("different request bytes")
    );
    let (code, replay) = submit(world.0, invalid);
    assert_eq!(code, 0);
    assert_eq!(copy(replay), bytes);
    assert_eq!(unsafe { disunity_response_free(replay) }.code, 0);
    assert_eq!(world.snapshot()["objects"], json!([]));
    assert_eq!(
        world.call(json!({"version":1,"requestId":"fresh","op":"object.create"}))["ok"],
        true
    );
}

#[test]
fn retained_semantic_errors_consume_the_same_non_evicting_count_budget() {
    let world = World::new();
    let first = br#"{"version":1,"requestId":"0","op":"object.get","object":0}"#;
    let error = call(world.0, first);
    assert_eq!(error["ok"], false);
    for id in 1..1024 {
        assert_eq!(
            world.call(json!({"version":1,"requestId":id.to_string(),"op":"missing"}))["ok"],
            false
        );
    }
    assert_eq!(
        submit(
            world.0,
            br#"{"version":1,"requestId":"full","op":"object.create"}"#
        ),
        (2, 0)
    );
    assert_eq!(call(world.0, first), error);
    assert_eq!(world.snapshot()["objects"], json!([]));
}

#[test]
fn utf8_id_limit_is_bytes_and_unadmitted_envelopes_do_not_claim_identity() {
    let world = World::new();
    let id = "é".repeat(64);
    assert_eq!(
        world.call(json!({"version":1,"requestId":id,"op":"time.get"}))["ok"],
        true
    );
    assert_eq!(
        world.call(json!({"version":1,"requestId":"é".repeat(65),"op":"object.create"}))["ok"],
        false
    );
    for invalid in [
        json!({"requestId":"after-invalid","op":"object.create"}),
        json!({"version":2,"requestId":"after-invalid","op":"object.create"}),
    ] {
        assert_eq!(world.call(invalid)["ok"], false);
    }
    assert_eq!(
        world.call(json!({"version":1,"requestId":"after-invalid","op":"object.create"}))["ok"],
        true
    );
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
}
