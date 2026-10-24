use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value, response::*};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { disunity_world_create(&mut handle) }.code, 0);
        Self(handle)
    }
    fn value(&self, request: Value) -> Value {
        request_value(self.0, request).unwrap()
    }
    fn bytes(&self, request: Value) -> Value {
        let response = request_bytes(self.0, &serde_json::to_vec(&request).unwrap()).unwrap();
        let bytes = response_bytes(response).unwrap();
        response_free(response).unwrap();
        assert!(response_bytes(response).is_err());
        serde_json::from_slice(&bytes).unwrap()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn both_query_transports_preserve_state_and_reject_invalid_handles_and_fields() {
    let w = World::new();
    let object = w.value(json!({"op":"object.create","name":"query"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let component = w.value(json!({"op":"component.add","object":object,"kind":"BoxCollider2D"}))["data"]["handle"].as_u64().unwrap();
    w.value(json!({"op":"input.set","keys":["Space"]}));
    w.value(json!({"op":"frame.begin","dt":0.1}));
    let before = w.value(json!({"op":"snapshot"}));
    for request in [
        json!({"op":"object.get","object":object}),
        json!({"op":"object.find","name":"query"}),
        json!({"op":"transform.get","object":object}),
        json!({"op":"component.get","component":component}),
        json!({"op":"component.list","object":object}),
        json!({"op":"snapshot"}),
        json!({"op":"time.get"}),
        json!({"op":"input.get"}),
        json!({"op":"events.read","clear":false}),
        json!({"op":"ui.layout"}),
        json!({"op":"physics.overlap","point":[0,0]}),
        json!({"op":"physics.raycast","origin":[-2,0],"direction":[1,0],"distance":4}),
    ] {
        let expected = w.value(request.clone());
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(w.bytes(request), expected);
    }
    for request in [
        json!({"op":"object.get","object":0}),
        json!({"op":"transform.get","object":0}),
        json!({"op":"component.get","component":0}),
        json!({"op":"component.list","object":0}),
        json!({"op":"ui.rect","object":object}),
        json!({"op":"time.get","invented":true}),
    ] {
        let expected = w.value(request.clone());
        assert_eq!(expected["ok"], false, "{expected}");
        assert_eq!(w.bytes(request), expected);
    }
    assert_eq!(w.bytes(json!({"op":"snapshot"})), before);
}

#[test]
fn query_failures_do_not_advance_random_or_drain_contact_events() {
    let w = World::new();
    for _ in 0..2 {
        let object = w.value(json!({"op":"object.create"}))["data"]["handle"]
            .as_u64()
            .unwrap();
        w.value(json!({"op":"component.add","object":object,"kind":"Rigidbody2D"}));
        w.value(json!({"op":"component.add","object":object,"kind":"BoxCollider2D","fields":{"is_trigger":true}}));
    }
    w.value(json!({"op":"physics.step","dt":0.02}));
    let events = w.bytes(json!({"op":"events.read","clear":false}));
    assert!(!events["data"].as_array().unwrap().is_empty());
    w.bytes(json!({"op":"random.seed","seed":42}));
    let range = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    let first = w.bytes(range.clone());
    let second = w.bytes(range.clone());
    assert_ne!(first, second);
    w.bytes(json!({"op":"random.seed","seed":42}));
    assert_eq!(
        w.bytes(json!({"op":"component.get","component":0}))["ok"],
        false
    );
    assert_eq!(w.bytes(range), first);
    assert_eq!(w.bytes(json!({"op":"events.read","clear":false})), events);
    assert_eq!(w.bytes(json!({"op":"events.read"})), events);
    assert_eq!(w.bytes(json!({"op":"events.read"}))["data"], json!([]));
}

#[test]
fn oversized_read_response_and_mutation_response_preserve_world_state() {
    let w = World::new();
    let object = w.value(json!({"op":"object.create","name":"original"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let padding = "x".repeat(512 * 1024);
    for _ in 0..33 {
        assert_eq!(w.bytes(json!({"op":"component.add","object":object,"kind":"Script","fields":{"padding":padding}}))["ok"], true);
    }
    let before = w.value(json!({"op":"snapshot"}));
    for request in [
        json!({"op":"object.get","object":object}),
        json!({"op":"snapshot"}),
        json!({"op":"object.set","object":object,"name":"rolled back"}),
    ] {
        let error = w.bytes(request);
        assert_eq!(error["ok"], false);
        assert_eq!(
            error["error"]["message"],
            "response budget exceeded; operation rolled back"
        );
        assert_eq!(w.value(json!({"op":"snapshot"})), before);
    }
    assert_eq!(w.bytes(json!({"op":"time.get"}))["ok"], true);
}

#[test]
fn repeated_invalid_reads_release_responses_and_preserve_full_response_capacity() {
    let w = World::new();
    let other = World::new();
    let foreign = other.value(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    let stale = w.value(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    w.bytes(json!({"op":"object.destroy","object":stale}));
    let replacement = w.value(json!({"op":"object.create"}))["data"]["handle"]
        .as_u64()
        .unwrap();
    assert_ne!(replacement, stale);
    let before = w.bytes(json!({"op":"snapshot"}));
    let mut errors = 0;
    for _ in 0..31 {
        for object in [foreign, stale] {
            let error = w.bytes(json!({"op":"object.get","object":object}));
            assert_eq!(error["ok"], false);
            assert_eq!(error["error"]["code"], "invalid_request");
            errors += 1;
        }
        assert_eq!(w.bytes(json!({"op":"snapshot"})), before);
    }
    assert_eq!(errors, 62);
    let responses: Vec<_> = (0..MAX_RESPONSES)
        .map(|_| request_bytes(w.0, br#"{"op":"time.get"}"#).unwrap())
        .collect();
    assert_eq!(request_bytes(w.0, br#"{"op":"time.get"}"#), Err(2));
    for response in responses {
        response_free(response).unwrap();
    }
    assert_eq!(w.bytes(json!({"op":"snapshot"})), before);
}
