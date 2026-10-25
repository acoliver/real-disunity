use super::*;

fn reject_unchanged(w: &World, request: Value) {
    let before = w.snapshot();
    assert_eq!(w.call(request)["ok"], false);
    assert_eq!(w.snapshot(), before);
}

fn claim_cleanup(w: &World, host: &Value, frame: u64, tick: u64) -> Value {
    let context = phase(w, host, "Cleanup", frame, tick);
    ok(
        w,
        json!({"op":"phase.claim","phase":"Cleanup","context":context}),
    );
    context
}

#[test]
fn cleanup_from_idle_grants_writes_once_then_closes_without_advancing_time() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    let object = ok(&w, json!({"op":"object.create","name":"cleanup target"}))["handle"].clone();
    run(&w, &host, "Initialize", 0, 0);
    let time = ok(&w, json!({"op":"time.get"}));
    let input = ok(&w, json!({"op":"input.get","phase":"fixed"}));
    let context = phase(&w, &host, "Cleanup", 0, 0);
    reject_unchanged(&w, json!({"op":"object.destroy","object":object}));
    reject_unchanged(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Cleanup","context":context}),
    );
    ok(&w, json!({"op":"object.destroy","object":object}));
    assert_eq!(ok(&w, json!({"op":"object.alive","object":object})), false);
    reject_unchanged(
        &w,
        json!({"op":"phase.claim","phase":"Cleanup","context":context}),
    );
    let closed = ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    assert_eq!(closed["stage"], "Closed");
    assert_eq!(closed["active_phase"], Value::Null);
    assert_eq!(ok(&w, json!({"op":"time.get"})), time);
    assert_eq!(ok(&w, json!({"op":"input.get","phase":"fixed"})), input);
    for request in [
        json!({"op":"object.create"}),
        json!({"op":"phase.finish","host":host,"context":context}),
        json!({"op":"phase.open","host":host,"phase":"Cleanup","frame":0,"fixed_tick":0}),
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
    ] {
        reject_unchanged(&w, request);
    }
}

#[test]
fn cleanup_can_dispose_preinitialize_setup_without_initializing() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    let context = claim_cleanup(&w, &host, 0, 0);
    ok(&w, json!({"op":"object.create","name":"preinit cleanup"}));
    let closed = ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    assert_eq!(closed["stage"], "Closed");
    assert_eq!(closed["frame"], 0);
    assert_eq!(closed["fixed_tick"], 0);
    assert_eq!(closed["claims"], 1);
}

#[test]
fn cancelling_cleanup_restores_origin_and_revokes_context() {
    let _table = crate::response_table::exclusive();
    for initialized in [false, true] {
        let w = World::new();
        let host = open(&w);
        if initialized {
            run(&w, &host, "Initialize", 0, 0);
        }
        let before = ok(&w, json!({"op":"session.status"}));
        let context = phase(&w, &host, "Cleanup", 0, 0);
        let cancelled = ok(
            &w,
            json!({"op":"phase.cancel","host":host,"context":context}),
        );
        assert_eq!(cancelled["stage"], before["stage"]);
        reject_unchanged(
            &w,
            json!({"op":"phase.claim","phase":"Cleanup","context":context}),
        );
        let fresh = claim_cleanup(&w, &host, 0, 0);
        assert_ne!(fresh["token"], context["token"]);
        reject_unchanged(&w, json!({"op":"phase.cancel","host":host,"context":fresh}));
        ok(&w, json!({"op":"phase.finish","host":host,"context":fresh}));
    }
}

#[test]
fn cleanup_rejects_live_roots_and_active_frames_without_effects() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    let request = json!({"op":"phase.open","host":host,"phase":"Cleanup","frame":0,"fixed_tick":0});
    let initialize = phase(&w, &host, "Initialize", 0, 0);
    reject_unchanged(&w, request.clone());
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":initialize}),
    );
    reject_unchanged(&w, request.clone());
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":initialize}),
    );
    let context = claim_cleanup(&w, &host, 0, 0);
    for operation in [
        request,
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
        json!({"op":"session.configure","host":host,"time_scale":{"num":1,"den":1}}),
        json!({"op":"fixed.commit","host":host,"frame":0,"fixed_tick":1}),
    ] {
        reject_unchanged(&w, operation);
    }
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}

#[test]
fn cleanup_uses_committed_frame_and_tick_after_real_fixed_work() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    let cleanup = json!({"op":"phase.open","host":host,"phase":"Cleanup","frame":1,"fixed_tick":0});
    reject_unchanged(&w, cleanup.clone());
    run(&w, &host, "FixedUpdate", 1, 1);
    run(&w, &host, "FixedCoroutines", 1, 1);
    reject_unchanged(&w, cleanup.clone());
    ok(
        &w,
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
    );
    run(&w, &host, "Contacts", 1, 1);
    for name in ["Update", "UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
        run(&w, &host, name, 1, 1);
        let mut current = cleanup.clone();
        current["fixed_tick"] = json!(1);
        reject_unchanged(&w, current);
    }
    ok(&w, json!({"op":"frame.end","host":host,"frame":1}));
    reject_unchanged(&w, cleanup);
    let before = ok(&w, json!({"op":"time.get"}));
    let context = claim_cleanup(&w, &host, 1, 1);
    ok(
        &w,
        json!({"op":"object.create","name":"after frame cleanup"}),
    );
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    assert_eq!(ok(&w, json!({"op":"time.get"})), before);
}

#[test]
fn cleanup_requires_authentic_identity_and_cannot_be_transactional_or_replayed() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let other = World::new();
    let host = open(&w);
    open(&other);
    run(&w, &host, "Initialize", 0, 0);
    for request in [
        json!({"op":"phase.open","host":{"session":host["session"],"host_key":"wrong"},"phase":"Cleanup","frame":0,"fixed_tick":0}),
        json!({"op":"phase.open","host":host,"phase":"Cleanup","frame":1,"fixed_tick":0}),
        json!({"op":"phase.open","host":host,"phase":"Cleanup","frame":0,"fixed_tick":1}),
        json!({"version":1,"requestId":"cleanup","op":"phase.open","host":host,"phase":"Cleanup","frame":0,"fixed_tick":0}),
    ] {
        reject_unchanged(&w, request);
    }
    let context = phase(&w, &host, "Cleanup", 0, 0);
    reject_unchanged(
        &other,
        json!({"op":"phase.claim","phase":"Cleanup","context":context}),
    );
    let claim = json!({"op":"phase.claim","phase":"Cleanup","context":context});
    reject_unchanged(
        &w,
        json!({"version":1,"op":"transaction","operations":[claim]}),
    );
    ok(&w, claim);
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}
