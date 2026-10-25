#[path = "support/response_table.rs"]
mod response_table;
use serde_json::json;
use worldrunner_disunity_ffi::{abi::*, request_value, response::*};
#[test]
fn exhausted_response_budget_rejects_before_mutating_and_recovers_after_free() {
    let _table = crate::response_table::exclusive();
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    let mut responses = Vec::new();
    for _ in 0..MAX_RESPONSES {
        responses.push(request_bytes(world, br#"{"op":"snapshot"}"#).unwrap());
    }
    assert_eq!(request_bytes(world, br#"{"op":"object.create"}"#), Err(2));
    assert_eq!(request_bytes(world, br#"{"op":"time.get"}"#), Err(2));
    assert_eq!(
        request_bytes(world, br#"{"op":"object.get","object":0}"#),
        Err(2)
    );
    assert_eq!(
        request_value(world, json!({"op":"snapshot"})).unwrap()["data"]["objects"],
        json!([])
    );
    for response in responses {
        response_free(response).unwrap();
    }
    let response = request_bytes(world, br#"{"op":"object.create"}"#).unwrap();
    response_free(response).unwrap();
    assert_eq!(
        request_value(world, json!({"op":"snapshot"})).unwrap()["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    unsafe {
        disunity_world_destroy(world);
    }
    assert_eq!(request_bytes(world, br#"{"op":"snapshot"}"#), Err(1));
}
