#[path = "readiness/boundaries.rs"]
mod boundaries;
#[path = "readiness/failures.rs"]
mod failures;
mod support;
use serde_json::{Value, json};
use support::World;

fn ok(w: &World, r: Value) -> Value {
    let v = w.call(r);
    assert_eq!(v["ok"], true, "{v}");
    v["data"].clone()
}
fn open(w: &World) -> (Value, Value) {
    let v = ok(
        w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    let host = json!({"session":v["session"],"host_key":v["host_key"]});
    let c = claim(w, &host, "Initialize", 0, 0);
    (host, c)
}
fn claim(w: &World, host: &Value, phase: &str, frame: u64, tick: u64) -> Value {
    let c = ok(
        w,
        json!({"op":"phase.open","host":host,"phase":phase,"frame":frame,"fixed_tick":tick}),
    )["context"]
        .clone();
    ok(w, json!({"op":"phase.claim","phase":phase,"context":c}));
    c
}
fn finish(w: &World, host: &Value, c: &Value) {
    ok(w, json!({"op":"phase.finish","host":host,"context":c}));
}
fn script(w: &World, c: &Value, callbacks: Value) -> (u64, u64) {
    let o = ok(w, json!({"op":"object.create"}))["handle"]
        .as_u64()
        .unwrap();
    let s = ok(w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"]
        .as_u64()
        .unwrap();
    ok(
        w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":callbacks}),
    );
    (o, s)
}
fn next(w: &World, c: &Value) -> Value {
    ok(w, json!({"op":"readiness.next","context":c}))
}
fn complete(w: &World, c: &Value, t: &Value, outcome: &str) {
    ok(
        w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(
        w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":outcome}),
    );
}
fn start(w: &World, c: &Value, s: u64) -> u64 {
    ok(w, json!({"op":"readiness.start","context":c,"component":s}))["routine"]
        .as_u64()
        .unwrap()
}
fn suspend(w: &World, c: &Value, r: u64, wait: Value) {
    let t = next(w, c);
    assert_eq!(t["kind"], "MoveNext");
    assert_eq!(t["routine"], r);
    complete(w, c, &t, "more");
    let t = next(w, c);
    assert_eq!(t["kind"], "Current");
    complete(w, c, &t, "ok");
    ok(
        w,
        json!({"op":"readiness.suspend","context":c,"routine":r,"wait":wait}),
    );
}
fn dynamic(w: &World, host: &Value, frame: u64, action: impl FnOnce(&Value)) {
    ok(
        w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":100}}),
    );
    let c = claim(w, host, "Update", frame, 0);
    finish(w, host, &c);
    let c = claim(w, host, "UpdateCoroutines", frame, 0);
    action(&c);
    finish(w, host, &c);
    for p in ["LateUpdate", "EndOfFrame"] {
        let c = claim(w, host, p, frame, 0);
        finish(w, host, &c);
    }
    ok(w, json!({"op":"frame.end","host":host,"frame":frame}));
}

#[test]
fn lifecycle_once_flags_attempts_and_fresh_peer_eligibility() {
    let w = World::new();
    let (host, c) = open(&w);
    let (_, a) = script(&w, &c, json!(["Awake", "OnEnable", "Start", "Update"]));
    let (owner, b) = script(&w, &c, json!(["Awake", "OnEnable", "Start", "Update"]));
    let t = next(&w, &c);
    assert_eq!(t["component"], a);
    assert_eq!(t["kind"], "Awake");
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(&w, json!({"op":"object.set","object":owner,"active":false}));
    ok(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"failed"}),
    );
    let t = next(&w, &c);
    assert_eq!(t["kind"], "OnEnable");
    complete(&w, &c, &t, "ok");
    assert!(next(&w, &c).is_null());
    ok(&w, json!({"op":"object.set","object":owner,"active":true}));
    for name in ["Awake", "OnEnable"] {
        let t = next(&w, &c);
        assert_eq!(t["component"], b);
        assert_eq!(t["kind"], name);
        complete(&w, &c, &t, "ok");
    }
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
}
#[test]
fn exact_context_linear_entry_and_idempotent_completion() {
    let w = World::new();
    let (host, c) = open(&w);
    script(&w, &c, json!(["Awake"]));
    let t = next(&w, &c);
    for key in [
        "world",
        "session",
        "frame",
        "fixed_tick",
        "phase",
        "sequence",
        "token",
    ] {
        let mut bad = c.clone();
        bad[key] = if ["session", "phase", "token"].contains(&key) {
            json!("forged")
        } else {
            json!(999)
        };
        assert_eq!(
            w.call(json!({"op":"readiness.enter","context":bad,"ticket":t["ticket"]}))["ok"],
            false
        );
    }
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":host,"context":c}))["ok"],
        false
    );
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    assert_eq!(
        w.call(json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}))["ok"],
        false
    );
    let done =
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"failed"});
    ok(&w, done.clone());
    ok(&w, done);
    assert_eq!(
        w.call(json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"}))
            ["ok"],
        false
    );
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
    assert_eq!(
        w.call(json!({"op":"readiness.next","context":c}))["ok"],
        false
    );
}
#[test]
fn paused_native_scaled_and_unscaled_waits() {
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    let scaled = start(&w, &c, s);
    suspend(
        &w,
        &c,
        scaled,
        json!({"kind":"scaled","seconds":{"num":1,"den":100}}),
    );
    let unscaled = start(&w, &c, s);
    suspend(
        &w,
        &c,
        unscaled,
        json!({"kind":"unscaled","seconds":{"num":1,"den":100}}),
    );
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"session.configure","host":host,"time_scale":{"num":0,"den":1}}),
    );
    dynamic(&w, &host, 1, |ctx| {
        let t = next(&w, ctx);
        assert_eq!(t["routine"], unscaled);
        complete(&w, ctx, &t, "done");
        let d = next(&w, ctx);
        assert_eq!(d["kind"], "Dispose");
        complete(&w, ctx, &d, "ok");
        assert!(next(&w, ctx).is_null());
    });
}
#[test]
fn cancellation_disposal_and_join_cycles_are_native() {
    let w = World::new();
    let (_, c) = open(&w);
    let (o, s) = script(&w, &c, json!([]));
    let a = start(&w, &c, s);
    suspend(&w, &c, a, Value::Null);
    let b = start(&w, &c, s);
    suspend(&w, &c, b, json!({"kind":"join","routine":a}));
    ok(&w, json!({"op":"readiness.cancel","context":c,"routine":a}));
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    assert_eq!(t["routine"], a);
    complete(&w, &c, &t, "failed");
    ok(&w, json!({"op":"object.set","object":o,"active":false}));
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    assert_eq!(t["routine"], b);
    complete(&w, &c, &t, "ok");
    assert!(next(&w, &c).is_null());
}
#[test]
fn primitive_requests_reject_replay_nested_controls_and_unknown_fields() {
    let w = World::new();
    let (_, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    for r in [
        json!({"version":1,"requestId":"no","op":"readiness.start","context":c,"component":s}),
        json!({"op":"readiness.start","context":c,"component":s,"extra":true}),
        json!({"version":1,"op":"transaction","operations":[{"op":"readiness.start","context":c,"component":s}]}),
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":["Unknown"]}),
    ] {
        assert_eq!(w.call(r)["ok"], false);
    }
    let r = start(&w, &c, s);
    suspend(&w, &c, r, Value::Null);
}

#[test]
fn stale_selection_is_revoked_before_enter_and_peer_mutation_is_preserved() {
    let w = World::new();
    let (_, c) = open(&w);
    let (o, _) = script(&w, &c, json!(["Awake"]));
    let t = next(&w, &c);
    ok(&w, json!({"op":"object.set","object":o,"active":false}));
    assert_eq!(
        ok(
            &w,
            json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]})
        )["execute"],
        false
    );
    assert!(next(&w, &c).is_null());
    assert_eq!(
        w.call(json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}))["ok"],
        false
    );
    assert_eq!(w.snapshot()["objects"][0]["active"], false);
}
#[test]
fn disposal_precedes_peer_updates_and_destroy_callback() {
    let w = World::new();
    let (host, c) = open(&w);
    let (o, s) = script(&w, &c, json!(["OnDisable", "OnDestroy"]));
    let (peer, _) = script(&w, &c, json!(["Update"]));
    let r = start(&w, &c, s);
    suspend(&w, &c, r, Value::Null);
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
    );
    let c = claim(&w, &host, "Update", 1, 0);
    ok(&w, json!({"op":"object.destroy","object":o}));
    let t = next(&w, &c);
    assert_eq!(t["kind"], "OnDisable");
    complete(&w, &c, &t, "ok");
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(&w, json!({"op":"object.set","object":peer,"active":false}));
    ok(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"}),
    );
    let t = next(&w, &c);
    assert_eq!(t["kind"], "OnDestroy");
    complete(&w, &c, &t, "failed");
    assert!(next(&w, &c).is_null());
}
#[test]
fn joins_require_next_dynamic_frame_and_disposal_and_reject_cycles() {
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    let a = start(&w, &c, s);
    suspend(&w, &c, a, Value::Null);
    let b = start(&w, &c, s);
    suspend(&w, &c, b, json!({"kind":"join","routine":a}));
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"session.configure","host":host,"time_scale":{"num":0,"den":1}}),
    );
    dynamic(&w, &host, 1, |c| {
        let t = next(&w, c);
        assert_eq!(t["routine"], a);
        complete(&w, c, &t, "more");
        let t = next(&w, c);
        complete(&w, c, &t, "ok");
        for target in [a, b, 999] {
            assert_eq!(w.call(json!({"op":"readiness.suspend","context":c,"routine":a,"wait":{"kind":"join","routine":target}}))["ok"],false);
        }
        ok(
            &w,
            json!({"op":"readiness.suspend","context":c,"routine":a,"wait":null}),
        );
        ok(&w, json!({"op":"readiness.cancel","context":c,"routine":a}));
        let t = next(&w, c);
        assert_eq!(t["kind"], "Dispose");
        complete(&w, c, &t, "ok");
        let t = next(&w, c);
        assert_eq!(t["routine"], b);
        complete(&w, c, &t, "more");
        let t = next(&w, c);
        complete(&w, c, &t, "ok");
        ok(
            &w,
            json!({"op":"readiness.suspend","context":c,"routine":b,"wait":{"kind":"join","routine":a}}),
        );
        assert!(next(&w, c).is_null());
    });
    dynamic(&w, &host, 2, |c| {
        let t = next(&w, c);
        assert_eq!(t["routine"], b);
        complete(&w, c, &t, "done");
        let t = next(&w, c);
        complete(&w, c, &t, "ok");
        ok(
            &w,
            json!({"op":"readiness.release","context":c,"routine":a}),
        );
        assert_eq!(
            w.call(json!({"op":"readiness.cancel","context":c,"routine":a}))["ok"],
            false
        );
    });
}
#[test]
fn caller_cannot_supply_internal_wait_deadline_or_frame() {
    let w = World::new();
    let (_, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    let r = start(&w, &c, s);
    let t = next(&w, &c);
    complete(&w, &c, &t, "more");
    let t = next(&w, &c);
    complete(&w, &c, &t, "ok");
    for wait in [
        json!({"kind":"next","frame":0}),
        json!({"kind":"fixed","tick":0}),
        json!({"kind":"scaled","seconds":{"num":0,"den":1},"frame":0}),
        json!({"kind":"predicate"}),
        json!({"kind":"scaled","seconds":{"num":1,"den":1000000000001u64}}),
    ] {
        assert_eq!(
            w.call(json!({"op":"readiness.suspend","context":c,"routine":r,"wait":wait}))["ok"],
            false
        );
    }
    ok(
        &w,
        json!({"op":"readiness.suspend","context":c,"routine":r,"wait":null}),
    );
}
