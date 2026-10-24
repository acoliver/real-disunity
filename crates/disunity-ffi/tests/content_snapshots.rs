#[path = "content_snapshots/authority.rs"]
mod authority;
#[path = "content_snapshots/coverage.rs"]
mod coverage;
#[path = "content_snapshots/retention.rs"]
mod retention;
mod support;
use serde_json::{Value, json};
use support::World;

fn ok(w: &World, r: Value) -> Value {
    let v = w.call(r);
    assert_eq!(v["ok"], true, "{v}");
    v["data"].clone()
}
fn setup(w: &World) -> (Value, Value) {
    let h = ok(
        w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    let h = json!({"session":h["session"],"host_key":h["host_key"]});
    let c = ok(
        w,
        json!({"op":"phase.open","host":h,"phase":"Initialize","frame":0,"fixed_tick":0}),
    )["context"]
        .clone();
    ok(
        w,
        json!({"op":"phase.claim","phase":"Initialize","context":c}),
    );
    (h, c)
}
fn idle(w: &World, h: &Value, c: &Value) {
    ok(w, json!({"op":"phase.finish","host":h,"context":c}));
}
fn capture(w: &World, h: &Value) -> Value {
    ok(w, json!({"op":"snapshot.capture","host":h}))
}
fn valid(w: &World, h: &Value, s: &Value, current: bool) -> Value {
    w.call(json!({"op":"snapshot.validate","host":h,"stamp":s,"require_current":current}))
}
fn revision(w: &World) -> u64 {
    ok(w, json!({"op":"session.status"}))["content_revision"]
        .as_u64()
        .unwrap()
}

#[test]
fn captures_are_host_bound_retained_and_do_not_self_invalidate() {
    let w = World::new();
    let (h, c) = setup(&w);
    let o = ok(&w, json!({"op":"object.create","name":"original"}))["handle"].clone();
    idle(&w, &h, &c);
    assert_eq!(w.snapshot()["objects"][0]["handle"], o);
    let a = capture(&w, &h);
    let b = capture(&w, &h);
    assert_ne!(a["stamp"]["snapshot_id"], b["stamp"]["snapshot_id"]);
    assert_eq!(a["snapshot"]["objects"][0]["handle"], o);
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
    assert_eq!(a["stamp"]["content_revision"], revision(&w));
    for request in [
        json!({"op":"snapshot"}),
        json!({"op":"session.status"}),
        json!({"op":"events.read","clear":false}),
    ] {
        ok(&w, request);
    }
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
    ok(
        &w,
        json!({"op":"snapshot.release","host":h,"stamp":b["stamp"]}),
    );
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
    assert_eq!(valid(&w, &h, &b["stamp"], false)["ok"], false);
    ok(
        &w,
        json!({"op":"session.configure","host":h,"time_scale":{"num":0,"den":1}}),
    );
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], false);
    assert_eq!(valid(&w, &h, &a["stamp"], false)["data"]["current"], false);
}

#[test]
fn content_revision_covers_mutations_queries_replay_and_atomic_rollback() {
    let w = World::new();
    let start = revision(&w);
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    assert_eq!(revision(&w), start + 1);
    for request in [
        json!({"op":"transform.set","object":o,"position":[1,2,3]}),
        json!({"op":"component.add","object":o,"kind":"Rigidbody2D"}),
        json!({"op":"physics.configure","gravity":[0,-1]}),
        json!({"op":"random.seed","seed":123}),
        json!({"op":"random.range","min":0,"max":1}),
        json!({"op":"input.set","keys":[]}),
        json!({"op":"time.set","time_scale":0}),
    ] {
        let before = revision(&w);
        ok(&w, request);
        assert_eq!(revision(&w), before + 1);
    }
    let request = json!({"version":1,"op":"transaction","requestId":"once","operations":[{"op":"random.range","min":0,"max":1},{"op":"object.set","object":o,"name":"changed"}]});
    let before = revision(&w);
    let first = ok(&w, request.clone());
    assert_eq!(revision(&w), before + 1);
    assert_eq!(ok(&w, request), first);
    assert_eq!(revision(&w), before + 1);
    let before = revision(&w);
    assert_eq!(w.call(json!({"version":1,"op":"transaction","operations":[{"op":"random.seed","seed":999},{"op":"object.set","object":0}]}))["ok"], false);
    assert_eq!(revision(&w), before);
    ok(
        &w,
        json!({"version":1,"op":"transaction","operations":[{"op":"time.get"},{"op":"snapshot"}]}),
    );
    assert_eq!(revision(&w), before);
}
