#[path = "phase_contexts/cleanup.rs"]
mod cleanup;
#[path = "phase_contexts/schedule.rs"]
mod schedule;
mod support;
use serde_json::{Value, json};
use support::World;

fn ok(w: &World, request: Value) -> Value {
    let response = w.call(request);
    assert_eq!(response["ok"], true, "{response}");
    response["data"].clone()
}
fn phase(w: &World, host: &Value, name: &str, frame: u64, tick: u64) -> Value {
    ok(
        w,
        json!({"op":"phase.open","host":host,"phase":name,"frame":frame,"fixed_tick":tick}),
    )["context"]
        .clone()
}
fn run(w: &World, host: &Value, name: &str, frame: u64, tick: u64) {
    let context = phase(w, host, name, frame, tick);
    ok(
        w,
        json!({"op":"phase.claim","phase":name,"context":context}),
    );
    ok(
        w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}
fn open(w: &World) -> Value {
    let data = ok(
        w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    json!({"session":data["session"],"host_key":data["host_key"]})
}
#[test]
fn desktop_exact_time_and_prescript_fixed_input_combined() {
    let w = World::new();
    ok(
        &w,
        json!({"op":"frame.begin_rational","dt":{"num":1,"den":60}}),
    );
    ok(
        &w,
        json!({"op":"frame.begin_rational","dt":{"num":1,"den":1000000000}}),
    );
    assert_eq!(
        ok(&w, json!({"op":"time.get"}))["rational"]["time"],
        json!({"num":50000003,"den":3000000000u64})
    );
    let strict = World::new();
    let host = open(&strict);
    ok(
        &strict,
        json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]}),
    );
    run(&strict, &host, "Initialize", 0, 0);
    ok(
        &strict,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    let context = phase(&strict, &host, "FixedUpdate", 1, 1);
    assert_eq!(ok(&strict, json!({"op":"time.get"}))["fixed_frame"], 0);
    assert_eq!(
        ok(&strict, json!({"op":"input.get","phase":"fixed"}))["keys"],
        json!(["W"])
    );
    ok(
        &strict,
        json!({"op":"phase.claim","phase":"FixedUpdate","context":context}),
    );
    ok(
        &strict,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    run(&strict, &host, "FixedCoroutines", 1, 1);
    ok(
        &strict,
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
    );
    assert_eq!(
        ok(&strict, json!({"op":"input.get","phase":"fixed"}))["down"],
        json!(["W"])
    );
    run(&strict, &host, "Contacts", 1, 1);
    for name in ["Update", "UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
        run(&strict, &host, name, 1, 1);
    }
    ok(&strict, json!({"op":"frame.end","host":host,"frame":1}));
}
#[test]
fn strict_session_claim_is_linear_and_controls_native_advance() {
    let w = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    let claim = json!({"op":"phase.claim","phase":"Initialize","context":context});
    ok(&w, claim.clone());
    let before = w.snapshot();
    assert_eq!(w.call(claim)["ok"], false);
    assert_eq!(
        w.call(json!({"op":"frame.begin_rational","dt":{"num":1,"den":50}}))["ok"],
        false
    );
    assert_eq!(w.snapshot(), before);
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}

#[test]
fn cancelled_unclaimed_fixed_scope_restores_input_and_clock() {
    let w = World::new();
    let host = open(&w);
    ok(
        &w,
        json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]}),
    );
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    let before_time = ok(&w, json!({"op":"time.get"}));
    let before_input = ok(&w, json!({"op":"input.get","phase":"fixed"}));
    let context = phase(&w, &host, "FixedUpdate", 1, 1);
    ok(
        &w,
        json!({"op":"phase.cancel","host":host,"context":context}),
    );
    assert_eq!(ok(&w, json!({"op":"time.get"})), before_time);
    assert_eq!(
        ok(&w, json!({"op":"input.get","phase":"fixed"})),
        before_input
    );
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"FixedUpdate","context":context}))["ok"],
        false
    );
    phase(&w, &host, "FixedUpdate", 1, 1);
    assert_eq!(
        ok(&w, json!({"op":"input.get","phase":"fixed"}))["down"],
        json!(["W"])
    );
}

#[test]
fn all_context_fields_and_outer_phase_are_native_assertions() {
    let w = World::new();
    let other = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    let before = w.snapshot();
    for (key, value) in [
        ("world", json!(other.0)),
        ("session", json!("wrong")),
        ("frame", json!(1)),
        ("fixed_tick", json!(1)),
        ("phase", json!("Update")),
        ("sequence", json!(900)),
        ("token", json!("wrong")),
    ] {
        let mut bad = context.clone();
        bad[key] = value;
        assert_eq!(
            w.call(json!({"op":"phase.claim","phase":"Initialize","context":bad}))["ok"],
            false
        );
        assert_eq!(w.snapshot(), before);
    }
    let mut missing = context.clone();
    missing.as_object_mut().unwrap().remove("token");
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"Initialize","context":missing}))["ok"],
        false
    );
    assert_eq!(
        other.call(json!({"op":"phase.claim","phase":"Initialize","context":context}))["ok"],
        false
    );
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"Update","context":context}))["ok"],
        false
    );
    assert_eq!(w.call(json!({"version":1,"requestId":"claim","op":"phase.claim","phase":"Initialize","context":context}))["ok"], false);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    assert_eq!(
        w.call(
            json!({"op":"phase.open","host":host,"phase":"Initialize","frame":0,"fixed_tick":0})
        )["ok"],
        false
    );
    assert_eq!(
        w.call(json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}))["ok"],
        false
    );
    ok(&w, json!({"op":"object.create","name":"callback mutation"}));
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"Initialize","context":context}))["ok"],
        false
    );
}

#[test]
fn direct_legacy_advance_and_transaction_cannot_bypass_authority() {
    let w = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    ok(&w, json!({"op":"random.seed","seed":42}));
    let draw = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    let expected = ok(&w, draw.clone());
    ok(&w, json!({"op":"random.seed","seed":42}));
    let before = w.snapshot();
    for request in [
        json!({"op":"frame.begin","dt":0.02}),
        json!({"op":"frame.begin_rational","dt":{"num":1,"den":50}}),
        json!({"op":"physics.step"}),
        json!({"op":"time.phase","phase":"fixed"}),
        json!({"op":"time.set","time_scale":2}),
        json!({"op":"time.set_rational","time_scale":{"num":2,"den":1}}),
        json!({"op":"lifetime.begin"}),
        json!({"op":"lifetime.end"}),
    ] {
        assert_eq!(w.call(request.clone())["ok"], false);
        assert_eq!(
            w.call(json!({"version":1,"op":"transaction","operations":[draw,request]}))["ok"],
            false
        );
        assert_eq!(w.snapshot(), before);
    }
    assert_ne!(
        unsafe { worldrunner_disunity_ffi::abi::disunity_world_step(w.0, 0.02, 1) }.code,
        0
    );
    assert_eq!(w.snapshot(), before);
    assert_eq!(ok(&w, draw), expected);
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}

#[test]
fn strict_control_credentials_affinity_pure_reads_and_snapshot_redaction() {
    let w = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    let before = w.snapshot();
    let mut bad = host.clone();
    bad["host_key"] = json!("incorrect");
    assert_eq!(
        w.call(json!({"op":"session.close","host":bad}))["ok"],
        false
    );
    let handle = w.0;
    let context2 = context.clone();
    let result = std::thread::spawn(move || {
        let read = support::call(handle, br#"{"op":"session.status"}"#);
        let claim = support::call(
            handle,
            &serde_json::to_vec(
                &json!({"op":"phase.claim","phase":"Initialize","context":context2}),
            )
            .unwrap(),
        );
        (read, claim)
    })
    .join()
    .unwrap();
    assert_eq!(result.0["ok"], true);
    assert_eq!(result.1["ok"], false);
    assert_eq!(w.snapshot(), before);
    let text = before.to_string();
    for secret in [
        host["host_key"].as_str().unwrap(),
        context["token"].as_str().unwrap(),
    ] {
        assert!(!text.contains(secret));
    }
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    ok(&w, json!({"op":"session.close","host":host}));
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":host,"context":context}))["ok"],
        false
    );
    assert_eq!(
        w.call(json!({"op":"session.open","profile":"native_phase_context_v1"}))["ok"],
        false
    );
}
