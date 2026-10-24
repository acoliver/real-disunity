mod support;
use serde_json::{Value, json};
use support::World;
use worldrunner_disunity_ffi::{request_value, response::*};

fn ok(w: &World, request: Value) -> Value {
    let result = w.call(request);
    assert_eq!(result["ok"], true, "{result}");
    result["data"].clone()
}
#[test]
fn handle_and_live_byte_admission_cannot_issue_a_receipt() {
    let w = World::new();
    let h = ok(
        &w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    let h = json!({"session":h["session"],"host_key":h["host_key"]});
    let c = ok(
        &w,
        json!({"op":"phase.open","host":h,"phase":"Initialize","frame":0,"fixed_tick":0}),
    )["context"]
        .clone();
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":c}),
    );
    let o = ok(&w, json!({"op":"object.create","name":"n".repeat(900_000)}))["handle"].clone();
    ok(&w, json!({"op":"phase.finish","host":h,"context":c}));
    let capture = serde_json::to_vec(&json!({"op":"snapshot.capture","host":h})).unwrap();
    let mut held = Vec::new();
    for _ in 0..MAX_RESPONSES {
        held.push(request_bytes(w.0, br#"{"op":"time.get"}"#).unwrap());
    }
    assert_eq!(request_bytes(w.0, &capture), Err(2));
    let status = request_value(w.0, json!({"op":"session.status"})).unwrap();
    assert_eq!(status["data"]["snapshot_receipts"], 0);
    for response in held.drain(..) {
        response_free(response).unwrap();
    }
    let get = serde_json::to_vec(&json!({"op":"object.get","object":o})).unwrap();
    while let Ok(response) = request_bytes(w.0, &get) {
        held.push(response);
    }
    assert!(held.len() < MAX_RESPONSES);
    assert_eq!(request_bytes(w.0, &capture), Err(2));
    assert_eq!(
        request_value(w.0, json!({"op":"session.status"})).unwrap(),
        status
    );
    for response in held {
        response_free(response).unwrap();
    }
    let a = ok(&w, json!({"op":"snapshot.capture","host":h}));
    assert_eq!(a["stamp"]["snapshot_id"], 1);
    assert_eq!(w.snapshot()["objects"][0]["handle"], o);
}
