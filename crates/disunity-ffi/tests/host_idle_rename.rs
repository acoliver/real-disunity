mod support;
use serde_json::{Value, json};
use support::World;

const PROFILE: &str = "native_host_idle_rename_v1";
fn ok(w: &World, request: Value) -> Value {
    let reply = w.call(request);
    assert_eq!(reply["ok"], true, "{reply}");
    reply["data"].clone()
}
fn open(w: &World, profile: &str) -> Value {
    let data = ok(w, json!({"op":"session.open","profile":profile}));
    json!({"session":data["session"],"host_key":data["host_key"]})
}
fn initialize(w: &World, host: &Value) {
    let context = ok(
        w,
        json!({"op":"phase.open","host":host,"phase":"Initialize","frame":0,"fixed_tick":0}),
    )["context"]
        .clone();
    ok(
        w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    ok(
        w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}
fn rename(host: &Value, object: u64) -> Value {
    json!({"version":1,"op":"host.mutate_idle","host":host,"request":{"op":"object.set","object":object,"name":"rust-mutated-again-1529"}})
}
#[test]
fn authenticated_idle_rename_preserves_schedule_time_input_and_ordinary_guard() {
    let w = World::new();
    let host = open(&w, PROFILE);
    let object = ok(&w, json!({"op":"object.create","name":"before"}))["handle"]
        .as_u64()
        .unwrap();
    let request = rename(&host, object);
    assert_eq!(w.call(request.clone())["ok"], false);
    initialize(&w, &host);
    let mut before = ok(&w, json!({"op":"session.status"}));
    let time = ok(&w, json!({"op":"time.get"}));
    let input = ok(&w, json!({"op":"input.get"}));
    assert_eq!(
        w.call(json!({"op":"object.set","object":object,"name":"unauthorized"}))["ok"],
        false
    );
    assert_eq!(ok(&w, request)["name"], "rust-mutated-again-1529");
    let after = ok(&w, json!({"op":"session.status"}));
    before["content_revision"] = json!(before["content_revision"].as_u64().unwrap() + 1);
    assert_eq!(after, before);
    assert_eq!(ok(&w, json!({"op":"time.get"})), time);
    assert_eq!(ok(&w, json!({"op":"input.get"})), input);
    assert!(
        !after
            .to_string()
            .contains(host["host_key"].as_str().unwrap())
    );
}
#[test]
fn old_profile_cannot_use_host_idle_permission() {
    let w = World::new();
    let host = open(&w, "native_phase_context_v1");
    let object = ok(&w, json!({"op":"object.create"}))["handle"]
        .as_u64()
        .unwrap();
    initialize(&w, &host);
    let before = w.snapshot();
    assert_eq!(w.call(rename(&host, object))["ok"], false);
    assert_eq!(w.snapshot(), before);
}
#[test]
fn invalid_idle_requests_have_no_effect() {
    let w = World::new();
    let host = open(&w, PROFILE);
    let object = ok(&w, json!({"op":"object.create"}))["handle"]
        .as_u64()
        .unwrap();
    initialize(&w, &host);
    let original = rename(&host, object);
    let mut variants = Vec::new();
    for (key, value) in [
        ("active", json!(false)),
        ("tag", json!("bad")),
        ("context", json!({})),
        ("requestId", json!("nested")),
    ] {
        let mut bad = original.clone();
        bad["request"][key] = value;
        variants.push(bad);
    }
    let mut wrong = original.clone();
    wrong["host"]["host_key"] = json!("wrong");
    variants.push(wrong);
    let mut replay = original.clone();
    replay["requestId"] = json!("replay");
    variants.push(replay);
    variants.push(json!({"version":1,"op":"transaction","operations":[original]}));
    let before = w.snapshot();
    for bad in variants {
        assert_eq!(w.call(bad)["ok"], false);
        assert_eq!(w.snapshot(), before);
    }
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":30}}),
    );
    let active = w.snapshot();
    assert_eq!(w.call(rename(&host, object))["ok"], false);
    assert_eq!(w.snapshot(), active);
}
