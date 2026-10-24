use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value, response::*};
#[test]
fn owned_response_reads_never_repeat_execution_and_survive_world_unload() {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    let response = request_bytes(world, br#"{"op":"object.create","name":"once"}"#).unwrap();
    let a = response_bytes(response).unwrap();
    let b = response_bytes(response).unwrap();
    assert_eq!(a, b);
    let snapshot = request_value(world, json!({"op":"snapshot"})).unwrap();
    assert_eq!(snapshot["data"]["objects"].as_array().unwrap().len(), 1);
    assert_eq!(unsafe { disunity_world_destroy(world) }.code, 0);
    let parsed: Value = serde_json::from_slice(&response_bytes(response).unwrap()).unwrap();
    assert_eq!(parsed["ok"], true);
    response_free(response).unwrap();
    assert!(response_bytes(response).is_err());
    assert!(response_free(response).is_err());
}
#[test]
fn malformed_and_unsupported_requests_do_not_execute() {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    assert!(request_bytes(world, b"{").is_err());
    let r = request_bytes(world, br#"{"op":"object.create","invented":true}"#).unwrap();
    let v: Value = serde_json::from_slice(&response_bytes(r).unwrap()).unwrap();
    assert_eq!(v["ok"], false);
    response_free(r).unwrap();
    assert!(
        request_value(world, json!({"op":"snapshot"})).unwrap()["data"]["objects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    unsafe {
        disunity_world_destroy(world);
    }
}

#[test]
fn c_transport_probes_and_small_buffers_never_repeat_or_partially_copy() {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    let request = br#"{"op":"object.create","name":"one"}"#;
    let mut response = 0;
    assert_eq!(
        unsafe { disunity_world_request(world, request.as_ptr(), request.len(), &mut response) }
            .code,
        0
    );
    let mut len = 0;
    assert_eq!(
        unsafe { disunity_response_copy(response, std::ptr::null_mut(), 0, &mut len) }.code,
        2
    );
    assert!(len > 0);
    let mut short = vec![0xA5; len - 1];
    let mut needed = 0;
    assert_eq!(
        unsafe { disunity_response_copy(response, short.as_mut_ptr(), short.len(), &mut needed) }
            .code,
        2
    );
    assert_eq!(needed, len);
    assert!(short.iter().all(|v| *v == 0xA5));
    let mut bytes = vec![0; len];
    assert_eq!(
        unsafe { disunity_response_copy(response, bytes.as_mut_ptr(), bytes.len(), &mut needed) }
            .code,
        0
    );
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["data"]["name"], "one");
    assert_eq!(
        request_value(world, json!({"op":"snapshot"})).unwrap()["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    assert_eq!(
        unsafe { disunity_response_copy(response, std::ptr::null_mut(), 0, &mut len) }.code,
        1
    );
    unsafe {
        disunity_world_destroy(world);
    }
}

#[test]
fn c_transport_reuses_slots_without_reviving_stale_responses_and_rejects_oversize() {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    let request = br#"{"op":"snapshot"}"#;
    let mut response = 0;
    assert_eq!(
        unsafe { disunity_world_request(world, request.as_ptr(), request.len(), &mut response) }
            .code,
        0
    );
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    let mut next = 0;
    assert_eq!(
        unsafe { disunity_world_request(world, request.as_ptr(), request.len(), &mut next) }.code,
        0
    );
    assert_ne!(next, response);
    assert_eq!(unsafe { disunity_response_free(next) }.code, 0);
    assert_eq!(
        unsafe {
            disunity_world_request(world, std::ptr::null(), MAX_REQUEST_BYTES + 1, &mut next)
        }
        .code,
        2
    );
    unsafe {
        disunity_world_destroy(world);
    }
}
