mod support;
use serde_json::{Value, json};
use support::World;

fn ok(w: &World, r: Value) -> Value {
    let v = w.call(r);
    assert_eq!(v["ok"], true, "{v}");
    v["data"].clone()
}
fn setup() -> (World, Value, u64) {
    let w = World::new();
    let h = ok(
        &w,
        json!({"op":"session.open","profile":"native_readiness_nested_start_v1"}),
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
    let s = ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"]
        .as_u64()
        .unwrap();
    ok(
        &w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":["Awake"]}),
    );
    (w, c, s)
}
fn next(w: &World, c: &Value) -> Value {
    ok(w, json!({"op":"readiness.next","context":c}))
}
fn enter(w: &World, c: &Value, t: &Value) {
    assert_eq!(
        ok(
            w,
            json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]})
        )["execute"],
        true
    );
}
fn complete(w: &World, c: &Value, t: &Value, outcome: &str) {
    ok(
        w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":outcome}),
    );
}
fn child(w: &World, c: &Value, s: u64) -> Value {
    ok(w, json!({"op":"readiness.start","context":c,"component":s}))
}

#[test]
fn child_suspension_restores_entered_parent_before_peer_selection() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let a = child(&w, &c, s);
    assert_eq!(a["immediate"]["kind"], "MoveNext");
    assert_eq!(
        w.call(
            json!({"op":"readiness.complete","context":c,"ticket":parent["ticket"],"outcome":"ok"})
        )["ok"],
        false
    );
    enter(&w, &c, &a["immediate"]);
    complete(&w, &c, &a["immediate"], "more");
    let current = next(&w, &c);
    assert_eq!(current["kind"], "Current");
    enter(&w, &c, &current);
    complete(&w, &c, &current, "ok");
    ok(
        &w,
        json!({"op":"readiness.suspend","context":c,"routine":a["routine"],"wait":null}),
    );
    assert_eq!(
        w.call(json!({"op":"readiness.next","context":c}))["ok"],
        false
    );
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn nested_movenext_restores_parent_current_pipeline_after_child_disposal() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let a = child(&w, &c, s);
    enter(&w, &c, &a["immediate"]);
    let b = child(&w, &c, s);
    enter(&w, &c, &b["immediate"]);
    complete(&w, &c, &b["immediate"], "failed");
    let dispose = next(&w, &c);
    assert_eq!(dispose["routine"], b["routine"]);
    assert_eq!(dispose["kind"], "Dispose");
    enter(&w, &c, &dispose);
    complete(&w, &c, &dispose, "failed");
    complete(&w, &c, &dispose, "failed");
    complete(&w, &c, &a["immediate"], "more");
    let current = next(&w, &c);
    assert_eq!(current["routine"], a["routine"]);
    enter(&w, &c, &current);
    complete(&w, &c, &current, "ok");
    ok(
        &w,
        json!({"op":"readiness.suspend","context":c,"routine":a["routine"],"wait":null}),
    );
    complete(&w, &c, &parent, "failed");
    assert!(next(&w, &c).is_null());
}

#[test]
fn nested_profile_descriptor_is_additive_and_native_only() {
    let w = World::new();
    let before = w.snapshot();
    let d = ok(&w, json!({"op":"describe"}));
    assert_eq!(w.snapshot(), before);
    assert!(
        d["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("native_readiness_nested_start_v1"))
    );
    assert_eq!(d["native_readiness"]["nested_stacks"], false);
    assert_eq!(d["native_readiness"]["managed_integration"], false);
    let n = &d["native_readiness_nested_start"];
    assert_eq!(n["max_child_depth"], 32);
    assert_eq!(n["managed_integration"], false);
    assert_eq!(n["yielded_ienumerator"], false);
    assert_eq!(n["start_inside_current"], false);
    assert_eq!(n["start_inside_dispose"], false);
}

#[path = "nested_start/bounds.rs"]
mod bounds;
