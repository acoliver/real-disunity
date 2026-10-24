use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value, store_statistics};

fn call(world: u64, value: Value) -> Value {
    let bytes = serde_json::to_vec(&value).unwrap();
    let mut handle = 0;
    assert_eq!(
        unsafe { disunity_world_request(world, bytes.as_ptr(), bytes.len(), &mut handle) }.code,
        0
    );
    let mut size = 0;
    assert_eq!(
        unsafe { disunity_response_copy(handle, std::ptr::null_mut(), 0, &mut size) }.code,
        2
    );
    let mut bytes = vec![0xA5; size];
    assert_eq!(
        unsafe { disunity_response_copy(handle, bytes.as_mut_ptr(), size - 1, &mut size) }.code,
        2
    );
    assert!(bytes.iter().all(|b| *b == 0xA5));
    for _ in 0..2 {
        assert_eq!(
            unsafe { disunity_response_copy(handle, bytes.as_mut_ptr(), bytes.len(), &mut size) }
                .code,
            0
        );
    }
    assert_eq!(unsafe { disunity_response_free(handle) }.code, 0);
    serde_json::from_slice(&bytes).unwrap()
}

fn assert_limits(data: &Value) {
    assert_eq!(data["limits"]["max_request_bytes"], 1 << 20);
    assert_eq!(data["limits"]["max_response_bytes"], 16 << 20);
    assert_eq!(data["limits"]["max_live_response_bytes"], 64 << 20);
    assert_eq!(data["limits"]["max_responses"], 1024);
}

fn assert_descriptor(data: &Value) {
    assert_eq!(data["native_abi_version"], 2);
    assert_eq!(data["native_protocol_versions"], json!([1]));
    assert_eq!(data["bridge_revisions"], json!([1]));
    assert_eq!(data["manifest_schema_versions"], json!([1]));
    assert_eq!(data["pointer_width"], usize::BITS);
    assert_eq!(
        data["endianness"],
        if cfg!(target_endian = "little") {
            "little"
        } else {
            "big"
        }
    );
    assert_limits(data);
    assert_eq!(data["clock_policy"], "float_accumulated");
    for missing in ["rational_clock", "phase_tokens"] {
        assert!(
            !data["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!(missing))
        );
    }
}

fn assert_random_unchanged(world: u64) {
    let actual = request_value(
        world,
        json!({"op":"random.range","min":0,"max":100000,"integer":true}),
    )
    .unwrap();
    request_value(world, json!({"op":"random.seed","seed":123})).unwrap();
    assert_eq!(
        request_value(
            world,
            json!({"op":"random.range","min":0,"max":100000,"integer":true})
        )
        .unwrap(),
        actual
    );
}

fn assert_invalid_describe(world: u64) {
    assert_eq!(
        call(world, json!({"op":"describe","invented":true}))["ok"],
        false
    );
    assert_eq!(
        call(world, json!({"version":2,"op":"describe"}))["ok"],
        false
    );
}

#[test]
fn describe_is_pure_global_and_live_world_cdecl_with_accurate_limits() {
    let initial = store_statistics();
    let global = call(0, json!({"version":1,"op":"describe"}));
    assert_eq!(global["ok"], true);
    assert_eq!(store_statistics(), initial);
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    request_value(world, json!({"op":"object.create","name":"purity-witness"})).unwrap();
    request_value(world, json!({"op":"random.seed","seed":123})).unwrap();
    let before = request_value(world, json!({"op":"snapshot"})).unwrap();
    let counts = store_statistics();
    let described = call(world, json!({"version":1,"op":"describe"}));
    assert_eq!(described["ok"], true, "live describe must be implemented");
    let data = &described["data"];
    assert_descriptor(data);
    assert_eq!(call(0, json!({"op":"describe"})), described);
    assert_eq!(
        request_value(0, json!({"op":"describe"})).unwrap(),
        described
    );
    assert_eq!(store_statistics(), counts);
    assert_eq!(
        request_value(world, json!({"op":"snapshot"})).unwrap(),
        before
    );
    assert_random_unchanged(world);
    assert_invalid_describe(world);
    assert_eq!(unsafe { disunity_world_destroy(world) }.code, 0);
    let bytes = br#"{"op":"describe"}"#;
    let mut response = 0;
    assert_eq!(
        unsafe { disunity_world_request(world, bytes.as_ptr(), bytes.len(), &mut response) }.code,
        1
    );
    assert_eq!(response, 0);
    assert_eq!(call(0, json!({"op":"describe"})), described);
    assert_eq!(store_statistics().0, counts.0 - 1);
}
