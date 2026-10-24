#[path = "terminal_cleanup/admission.rs"]
mod admission;
#[path = "terminal_cleanup/budgets.rs"]
mod budgets;
#[path = "terminal_cleanup/readiness.rs"]
mod readiness;
mod support;
use serde_json::{Value, json};
use support::World;

fn ok(w: &World, request: Value) -> Value {
    let result = w.call(request);
    assert_eq!(result["ok"], true, "{result}");
    result["data"].clone()
}
fn open(w: &World) -> Value {
    let h = ok(
        w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    json!({"session":h["session"],"host_key":h["host_key"]})
}
fn issue(w: &World, h: &Value, phase: &str) -> Value {
    let s = ok(w, json!({"op":"session.status"}));
    let tick = s["fixed_tick"].as_u64().unwrap()
        + u64::from(matches!(phase, "FixedUpdate" | "FixedCoroutines"));
    ok(
        w,
        json!({"op":"phase.open","host":h,"phase":phase,"frame":s["frame"],"fixed_tick":tick}),
    )["context"]
        .clone()
}
fn claim(w: &World, c: &Value) {
    ok(
        w,
        json!({"op":"phase.claim","phase":c["phase"],"context":c}),
    );
}
fn finish(w: &World, h: &Value, c: &Value) -> Value {
    ok(w, json!({"op":"phase.finish","host":h,"context":c}))
}
fn run(w: &World, h: &Value, phase: &str) {
    let c = issue(w, h, phase);
    claim(w, &c);
    finish(w, h, &c);
}
fn begin(w: &World, h: &Value) -> Value {
    ok(
        w,
        json!({"op":"session.begin_cleanup","host":h,"reason":"infrastructure_failure"}),
    )["context"]
        .clone()
}
fn reject(w: &World, request: Value) {
    let before = w.snapshot();
    let status = ok(w, json!({"op":"session.status"}));
    assert_eq!(w.call(request)["ok"], false);
    assert_eq!(w.snapshot(), before);
    assert_eq!(ok(w, json!({"op":"session.status"})), status);
}
fn frozen(w: &World) -> Value {
    json!({"time":ok(w,json!({"op":"time.get"})),
        "fixed":ok(w,json!({"op":"input.get","phase":"fixed"})),
        "dynamic":ok(w,json!({"op":"input.get","phase":"dynamic"}))})
}
fn at_stage(w: &World, h: &Value, stage: &str) -> Option<Value> {
    if stage == "preinit" {
        return None;
    }
    if stage == "Initialize" {
        return Some(issue(w, h, stage));
    }
    run(w, h, "Initialize");
    if stage == "Idle" {
        return None;
    }
    ok(
        w,
        json!({"op":"frame.open","host":h,"dt":{"num":3,"den":100}}),
    );
    for p in ["FixedUpdate", "FixedCoroutines"] {
        if stage == p {
            return Some(issue(w, h, p));
        }
        run(w, h, p);
    }
    if stage == "Commit" {
        return None;
    }
    ok(
        w,
        json!({"op":"fixed.commit","host":h,"frame":1,"fixed_tick":1}),
    );
    for p in [
        "Contacts",
        "Update",
        "UpdateCoroutines",
        "LateUpdate",
        "EndOfFrame",
    ] {
        if stage == p {
            return Some(issue(w, h, p));
        }
        run(w, h, p);
    }
    None
}

#[test]
fn terminal_freezes_actual_projections_at_every_open_stage() {
    for stage in [
        "preinit",
        "Initialize",
        "Idle",
        "FixedUpdate",
        "FixedCoroutines",
        "Commit",
        "Contacts",
        "Update",
        "UpdateCoroutines",
        "LateUpdate",
        "EndOfFrame",
        "End",
    ] {
        for claimed in [false, true] {
            let w = World::new();
            let h = open(&w);
            ok(
                &w,
                json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]}),
            );
            let old = at_stage(&w, &h, stage);
            if claimed && let Some(c) = &old {
                claim(&w, c);
            }
            let before = frozen(&w);
            let s = ok(&w, json!({"op":"session.status"}));
            let c = begin(&w, &h);
            assert_eq!(frozen(&w), before, "{stage}");
            assert_eq!(c["frame"], s["frame"]);
            assert_eq!(c["fixed_tick"], s["fixed_tick"]);
            assert_eq!(
                ok(&w, json!({"op":"session.status"}))["terminal_cleanup"],
                true
            );
            if let Some(old) = old {
                reject(
                    &w,
                    json!({"op":"phase.claim","phase":old["phase"],"context":old}),
                );
            }
            reject(&w, json!({"op":"phase.cancel","host":h,"context":c}));
            claim(&w, &c);
            assert_eq!(finish(&w, &h, &c)["stage"], "Closed");
            assert_eq!(frozen(&w), before);
            reject(&w, json!({"op":"session.begin_cleanup","host":h}));
        }
    }
}

#[test]
fn terminal_preserves_effects_rng_body_intent_and_flushes_deferred_lifetime() {
    let w = World::new();
    let h = open(&w);
    let o = ok(&w, json!({"op":"object.create","name":"accepted"}))["handle"].clone();
    let body = ok(
        &w,
        json!({"op":"component.add","object":o,"kind":"Rigidbody2D"}),
    )["handle"]
        .clone();
    run(&w, &h, "Initialize");
    ok(
        &w,
        json!({"op":"frame.open","host":h,"dt":{"num":3,"den":100}}),
    );
    let old = issue(&w, &h, "FixedUpdate");
    claim(&w, &old);
    ok(
        &w,
        json!({"op":"body.move","component":body,"position":[9,8]}),
    );
    ok(
        &w,
        json!({"op":"object.set","object":o,"name":"callback accepted"}),
    );
    ok(&w, json!({"op":"random.seed","seed":42}));
    let draw = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    let expected = ok(&w, draw.clone());
    ok(&w, json!({"op":"random.seed","seed":42}));
    let before = w.snapshot();
    let c = begin(&w, &h);
    claim(&w, &c);
    assert_eq!(ok(&w, draw), expected);
    assert_eq!(w.snapshot()["objects"], before["objects"]);
    ok(&w, json!({"op":"object.destroy","object":o}));
    assert_eq!(ok(&w, json!({"op":"object.alive","object":o})), true);
    finish(&w, &h, &c);
    assert_eq!(ok(&w, json!({"op":"object.alive","object":o})), false);
    assert_eq!(ok(&w, json!({"op":"time.get"}))["fixed_frame"], 0);
}

#[test]
fn terminal_descriptor_is_explicitly_native_only() {
    let w = World::new();
    let d = ok(&w, json!({"op":"describe"}));
    assert!(
        d["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("native_terminal_cleanup_v1"))
    );
    assert_eq!(d["native_terminal_cleanup"]["managed_integration"], false);
    assert_eq!(d["native_terminal_cleanup"]["recovery_roots"], 1);
}
