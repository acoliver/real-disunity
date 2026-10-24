#[path = "contextual_batches/admission.rs"]
mod admission;
#[path = "contextual_batches/codec.rs"]
mod codec;
mod support;
use serde_json::{Value, json};
use support::World;
const PROFILE: &str = "native_contextual_lifecycle_batch_v1";
fn ok(w: &World, r: Value) -> Value {
    let v = w.call(r);
    assert_eq!(v["ok"], true, "{}", v["error"]);
    v["data"].clone()
}
#[test]
fn contextual_cleanup_profile_is_explicitly_available() {
    let w = World::new();
    assert_eq!(
        ok(&w, json!({"op":"session.open","profile":PROFILE}))["profile"],
        PROFILE
    );
}
fn claim(w: &World, host: &Value, phase: &str) -> Value {
    let c = ok(
        w,
        json!({"op":"phase.open","host":host,"phase":phase,"frame":0,"fixed_tick":0}),
    )["context"]
        .clone();
    ok(w, json!({"op":"phase.claim","phase":phase,"context":c}));
    c
}
fn setup(w: &World, count: usize) -> (Value, Value, Vec<u64>) {
    let s = ok(w, json!({"op":"session.open","profile":PROFILE}));
    let host = json!({"session":s["session"],"host_key":s["host_key"]});
    let c = claim(w, &host, "Initialize");
    let o = ok(w, json!({"op":"object.create"}))["handle"].clone();
    let mut components = Vec::new();
    for _ in 0..count {
        let component = ok(w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"]
            .as_u64()
            .unwrap();
        ok(
            w,
            json!({"op":"readiness.register","context":c,"component":component,"callbacks":["Awake","OnEnable","OnDisable","OnDestroy"]}),
        );
        components.push(component);
    }
    for _ in 0..count * 2 {
        let t = ok(w, json!({"op":"readiness.next","context":c}));
        assert_eq!(
            ok(
                w,
                json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]})
            )["execute"],
            true
        );
        ok(
            w,
            json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"}),
        );
    }
    assert!(ok(w, json!({"op":"readiness.next","context":c})).is_null());
    ok(w, json!({"op":"phase.finish","host":host,"context":c}));
    let c = claim(w, &host, "Cleanup");
    (host, c, components)
}
#[test]
fn cleanup_reserves_current_native_lifecycle_candidates() {
    let w = World::new();
    let (_, c, components) = setup(&w, 2);
    let e = ok(&w, json!({"op":"callbacks.next","context":c}));
    assert_eq!(e["callbacks"].as_array().unwrap().len(), 4);
    assert_eq!(e["callbacks"][0]["component"], components[0]);
    assert_eq!(e["callbacks"][0]["kind"], "OnDisable");
    assert_eq!(e["callbacks"][2]["kind"], "OnDestroy");
}
fn batch(w: &World, c: &Value) -> Value {
    ok(w, json!({"op":"callbacks.next","context":c}))
}
fn accept(w: &World, c: &Value, e: &Value) {
    ok(w, json!({"op":"callbacks.accept","context":c,"envelope":e}));
}
fn inspect(w: &World, c: &Value, e: &Value) -> Value {
    ok(
        w,
        json!({"op":"callbacks.inspect","context":c,"batch":e["context"]["callback_batch"]}),
    )
}
fn enter(w: &World, c: &Value, e: &Value, index: usize) -> Value {
    ok(
        w,
        json!({"op":"callbacks.enter","context":c,"batch":e["context"]["callback_batch"],"index":index}),
    )
}
fn complete(w: &World, c: &Value, t: &Value, outcome: &str) {
    ok(
        w,
        json!({"op":"readiness.complete","context":c,"ticket":t["decision"]["ticket"],"outcome":outcome}),
    );
}
fn finish_batch(w: &World, c: &Value, e: &Value) {
    ok(
        w,
        json!({"op":"callbacks.finish","context":c,"batch":e["context"]["callback_batch"]}),
    );
}
#[test]
fn one_exact_4096_record_envelope_enters_and_completes_native_tickets_once() {
    let w = World::new();
    let (host, c, components) = setup(&w, 2048);
    let e = batch(&w, &c);
    let records = e["callbacks"].as_array().unwrap();
    assert_eq!(records.len(), 4096);
    let encoded = serde_json::to_vec(&e).unwrap().len();
    assert!(encoded <= 1 << 20);
    assert_eq!(e["context"]["time"], json!({"numerator":0,"denominator":1}));
    assert_eq!(e["context"]["root_phase"], "Cleanup");
    assert_eq!(e["context"]["phase"], "destroy");
    assert_eq!(w.snapshot()["time"]["frame"], 0);
    accept(&w, &c, &e);
    drain_exact(&w, &c, &e, &components);
    assert_eq!(inspect(&w, &c, &e)["cursor"], 4096);
    finish_batch(&w, &c, &e);
    assert!(batch(&w, &c).is_null());
    let status = ok(&w, json!({"op":"phase.finish","host":host,"context":c}));
    assert_eq!(status["stage"], "Closed");
    println!(
        "native_only records=4096 serial_initialize_entries=4096 cleanup_entries=4096 envelope_bytes={encoded} managed_entries=0"
    );
}
#[test]
fn whole_envelope_forgery_and_linear_bypass_leave_reservation_unchanged() {
    let w = World::new();
    let (host, c, _) = setup(&w, 2);
    let e = batch(&w, &c);
    let before = inspect(&w, &c, &e);
    reject_forged_envelopes(&w, &c, &e);
    for r in [
        json!({"op":"readiness.next","context":c}),
        json!({"op":"readiness.enter","context":c,"ticket":1}),
        json!({"op":"phase.finish","host":host,"context":c}),
        json!({"version":1,"requestId":"replay","op":"callbacks.next","context":c}),
        json!({"version":1,"op":"transaction","operations":[{"op":"callbacks.next","context":c}]}),
    ] {
        assert_eq!(w.call(r)["ok"], false);
    }
    assert_eq!(inspect(&w, &c, &e), before);
    accept(&w, &c, &e);
    assert_eq!(
        w.call(json!({"op":"callbacks.accept","context":c,"envelope":e}))["ok"],
        false
    );
    assert_eq!(w.call(json!({"op":"callbacks.enter","context":c,"batch":e["context"]["callback_batch"],"index":1}))["ok"], false);
    let t = enter(&w, &c, &e, 0);
    assert_eq!(w.call(json!({"op":"callbacks.enter","context":c,"batch":e["context"]["callback_batch"],"index":0}))["ok"], false);
    complete(&w, &c, &t, "failed");
    complete(&w, &c, &t, "failed");
    assert_eq!(inspect(&w, &c, &e)["cursor"], 1);
}
#[test]
fn raw_overflow_escaped_duplicates_and_unknown_last_records_execute_nothing() {
    let w = World::new();
    let (_, c, _) = setup(&w, 1);
    let e = batch(&w, &c);
    let before = inspect(&w, &c, &e);
    let request = json!({"op":"callbacks.accept","context":c,"envelope":e});
    let mut over = request.clone();
    over["envelope"]["callbacks"] = json!(vec![e["callbacks"][0].clone(); 4097]);
    let (code, response) = support::submit(w.0, &serde_json::to_vec(&over).unwrap());
    assert_eq!(code, 4);
    assert_eq!(response, 0);
    let raw = serde_json::to_string(&request).unwrap();
    for malformed in [
        raw.replacen("\"binding\":", "\"b\\u0069nding\":1,\"binding\":", 1),
        raw.replacen(
            "\"op\":\"callbacks.accept\"",
            "\"op\":\"callbacks.accept\",\"op\":\"object.create\"",
            1,
        ),
        raw.replacen(
            "\"op\":\"callbacks.accept\"",
            "\"op\":\"object.create\",\"o\\u0070\":\"callbacks.accept\"",
            1,
        ),
    ] {
        assert_eq!(support::submit(w.0, malformed.as_bytes()), (4, 0));
    }
    let mut bad = request;
    bad["envelope"]["callbacks"][1]["payload"]["unknown"] = json!(true);
    assert_eq!(w.call(bad)["ok"], false);
    assert_eq!(inspect(&w, &c, &e), before);
    accept(&w, &c, &e);
    assert_eq!(enter(&w, &c, &e, 0)["execute"], true);
}
#[test]
fn removed_generation_retains_native_cleanup_and_failed_attempt_preserves_writes() {
    let w = World::new();
    let (_, c, components) = setup(&w, 2);
    let e = batch(&w, &c);
    accept(&w, &c, &e);
    let t = enter(&w, &c, &e, 0);
    ok(
        &w,
        json!({"op":"component.remove","component":components[1]}),
    );
    ok(
        &w,
        json!({"op":"component.set","component":components[0],"enabled":false}),
    );
    complete(&w, &c, &t, "failed");
    for i in 1..4 {
        let t = enter(&w, &c, &e, i);
        assert_eq!(t["execute"], true);
        complete(&w, &c, &t, "ok");
    }
    finish_batch(&w, &c, &e);
    assert!(batch(&w, &c).is_null());
}
#[test]
fn credentials_profiles_scope_and_stale_worlds_fail_closed() {
    let w = World::new();
    let (host, c, components) = setup(&w, 1);
    let e = batch(&w, &c);
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
            json!("foreign")
        } else {
            json!(42)
        };
        assert_eq!(
            w.call(json!({"op":"callbacks.next","context":bad}))["ok"],
            false
        );
    }
    for r in [
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
        json!({"op":"readiness.start","context":c,"component":components[0]}),
        json!({"op":"readiness.register","context":c,"component":components[0],"callbacks":[]}),
    ] {
        assert_eq!(w.call(r)["ok"], false);
    }
    let other = World::new();
    assert_eq!(
        other.call(json!({"op":"callbacks.accept","context":c,"envelope":e}))["ok"],
        false
    );
    drop(w);
    let reused = World::new();
    assert_eq!(
        reused.call(json!({"op":"callbacks.accept","context":c,"envelope":e}))["ok"],
        false
    );
}

fn drain_exact(w: &World, c: &Value, e: &Value, components: &[u64]) {
    let records = e["callbacks"].as_array().unwrap();
    for (i, record) in records.iter().enumerate() {
        assert_eq!(record["component"], components[i % 2048]);
        assert_eq!(
            record["kind"],
            if i < 2048 { "OnDisable" } else { "OnDestroy" }
        );
        let t = enter(w, c, e, i);
        assert_eq!(t["execute"], true);
        assert_eq!(t["decision"]["component"], record["component"]);
        assert_eq!(t["decision"]["kind"], record["kind"]);
        complete(w, c, &t, if i % 97 == 0 { "failed" } else { "ok" });
    }
}

fn reject_forged_envelopes(w: &World, c: &Value, e: &Value) {
    let before = inspect(w, c, e);
    for key in ["binding", "component", "kind", "payload", "extra"] {
        let mut bad = e.clone();
        bad["callbacks"][3][key] = json!("forged");
        assert_eq!(
            w.call(json!({"op":"callbacks.accept","context":c,"envelope":bad}))["ok"],
            false
        );
        assert_eq!(inspect(w, c, e), before);
    }
    for key in [
        "lease",
        "root_phase",
        "phase",
        "flags",
        "time",
        "dynamic_delta",
        "fixed_delta",
        "unscaled_delta",
        "callback_batch",
    ] {
        let mut bad = e.clone();
        bad["context"][key] = if key.ends_with("delta") {
            json!(1)
        } else {
            json!(0)
        };
        assert_eq!(
            w.call(json!({"op":"callbacks.accept","context":c,"envelope":bad}))["ok"],
            false
        );
    }
    let mut duplicate = e.clone();
    duplicate["callbacks"][3] = e["callbacks"][0].clone();
    assert_eq!(
        w.call(json!({"op":"callbacks.accept","context":c,"envelope":duplicate}))["ok"],
        false
    );
}
