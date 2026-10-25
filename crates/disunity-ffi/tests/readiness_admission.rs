#[path = "support/response_table.rs"]
mod response_table;
mod support;
use serde_json::{Value, json};
use support::World;
fn ok(w: &World, r: Value) -> Value {
    let v = w.call(r);
    assert_eq!(v["ok"], true, "{v}");
    v["data"].clone()
}
#[test]
fn response_admission_cannot_consume_callback_entry() {
    let _table = crate::response_table::exclusive();
    use worldrunner_disunity_ffi::response::{MAX_RESPONSES, request_bytes, response_free};
    let w = World::new();
    let h = ok(
        &w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    let host = json!({"session":h["session"],"host_key":h["host_key"]});
    let c = ok(
        &w,
        json!({"op":"phase.open","host":host,"phase":"Initialize","frame":0,"fixed_tick":0}),
    )["context"]
        .clone();
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":c}),
    );
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let s = ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    ok(
        &w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":["Awake"]}),
    );
    let t = ok(&w, json!({"op":"readiness.next","context":c}));
    let before = w.snapshot();
    let mut held = Vec::new();
    for _ in 0..MAX_RESPONSES {
        held.push(request_bytes(w.0, br#"{"op":"time.get"}"#).unwrap());
    }
    let bytes =
        serde_json::to_vec(&json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}))
            .unwrap();
    assert_eq!(request_bytes(w.0, &bytes), Err(2));
    for h in held {
        response_free(h).unwrap();
    }
    assert_eq!(w.snapshot(), before);
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"}),
    );
    assert!(ok(&w, json!({"op":"readiness.next","context":c})).is_null());
}
