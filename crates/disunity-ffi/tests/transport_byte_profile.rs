use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, store_statistics};

fn call(world: u64, request: Value) -> Value {
    let bytes = serde_json::to_vec(&request).unwrap();
    let mut response = 0;
    assert_eq!(
        unsafe { disunity_world_request(world, bytes.as_ptr(), bytes.len(), &mut response) }.code,
        0
    );
    let mut len = 0;
    assert_eq!(
        unsafe { disunity_response_copy(response, std::ptr::null_mut(), 0, &mut len) }.code,
        2
    );
    let mut output = vec![0; len];
    assert_eq!(
        unsafe { disunity_response_copy(response, output.as_mut_ptr(), output.len(), &mut len) }
            .code,
        0
    );
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn advertises_byte_profile_separately_without_claiming_managed_callback_integration() {
    let described = call(0, json!({"version":1,"op":"describe"}));
    let data = &described["data"];
    assert!(
        data["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("transport_byte_limits_v1"))
    );
    assert_eq!(
        data["transport_byte_limits_v1"]["managed_integration"],
        false
    );
    assert_eq!(
        data["transport_byte_limits_v1"]["callback_batch_attestation"],
        false
    );
    assert_eq!(
        data["transport_byte_limits_v1"]["limits"]["max_request_bytes"],
        1 << 20
    );
    assert_eq!(
        data["transport_byte_limits_v1"]["limits"]["max_response_bytes"],
        8 << 20
    );
    assert_eq!(
        data["transport_byte_limits_v1"]["limits"]["max_outstanding_bytes"],
        16 << 20
    );
    assert_eq!(data["limits"]["max_response_bytes"], 16 << 20);
    assert_eq!(data["limits"]["max_live_response_bytes"], 64 << 20);
    assert_eq!(store_statistics().0, 0);
}
