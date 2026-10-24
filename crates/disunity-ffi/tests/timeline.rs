mod support;
mod timeline {
    mod admission;
    mod clock;
    mod input;
}

use serde_json::{Value, json};
use support::World;

fn ok(w: &World, request: Value) -> Value {
    let response = w.call(request);
    assert_eq!(response["ok"], true, "{response}");
    response["data"].clone()
}
fn ratio(num: u64, den: u64) -> Value {
    json!({"num":num,"den":den})
}
fn begin(w: &World, num: u64, den: u64) -> Value {
    ok(w, json!({"op":"frame.begin_rational","dt":ratio(num,den)}))
}
fn select(w: &World) {
    ok(w, json!({"op":"time.set_rational"}));
}
fn key(id: u64, num: u64, den: u64, name: &str, pressed: bool) -> Value {
    json!({"event_id":id,"timestamp":ratio(num,den),"source":"os",
        "kind":"key","key":name,"pressed":pressed})
}
fn enqueue(w: &World, events: Vec<Value>) -> Value {
    ok(w, json!({"op":"input.events","events":events}))
}
fn input(w: &World, phase: &str) -> Value {
    ok(w, json!({"op":"input.get","phase":phase}))
}

#[test]
fn descriptor_names_only_implemented_timeline_capabilities() {
    let w = World::new();
    let d = ok(&w, json!({"op":"describe"}));
    let caps = d["capabilities"].as_array().unwrap();
    for cap in ["rational_time", "unscaled_time", "timestamped_input"] {
        assert!(caps.contains(&json!(cap)), "{d}");
    }
    assert!(!caps.contains(&json!("phase_tokens")));
    assert_eq!(d["native_abi_version"], 2);
    assert_eq!(d["clock_policy"], "float_accumulated");
    assert_eq!(d["rational_clock_policy"], "explicit_bounded_exact");
    assert_eq!(d["limits"]["max_input_events"], 4096);
}

#[test]
fn generation_and_affinity_cover_new_mutations_and_immutable_queries() {
    let w = World::new();
    select(&w);
    enqueue(&w, vec![key(1, 0, 1, "W", true)]);
    begin(&w, 1, 60);
    let handle = w.0;
    let before = input(&w, "dynamic");
    let read = std::thread::spawn(move || {
        let denied = support::call(handle, br#"{"op":"input.events","events":[]}"#);
        assert_eq!(denied["ok"], false);
        let read = support::call(handle, br#"{"op":"input.get","phase":"dynamic"}"#);
        assert_eq!(support::call(handle, br#"{"op":"time.get"}"#)["ok"], true);
        read["data"].clone()
    })
    .join()
    .unwrap();
    assert_eq!(read, before);
    drop(w);
    let fresh = World::new();
    assert_ne!(fresh.0, handle);
    let status = ok(&fresh, json!({"op":"input.status"}));
    assert_eq!(status["last_event_id"], 0);
    select(&fresh);
    enqueue(&fresh, vec![key(1, 0, 1, "W", true)]);
    assert!(worldrunner_disunity_ffi::request_value(handle, json!({"op":"time.get"})).is_err());
    let _ = fresh.snapshot();
}
