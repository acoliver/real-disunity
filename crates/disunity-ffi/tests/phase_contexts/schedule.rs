#[test]
fn nonclearing_events_are_available_before_contacts_claim_without_draining() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    ok(&w, json!({"op":"physics.configure","gravity":[0,0]}));
    for _ in 0..2 {
        let object = ok(&w, json!({"op":"object.create"}))["handle"].clone();
        ok(
            &w,
            json!({"op":"component.add","object":object,"kind":"Rigidbody2D","fields":{"body_type":"kinematic"}}),
        );
        ok(
            &w,
            json!({"op":"component.add","object":object,"kind":"BoxCollider2D","fields":{"is_trigger":true}}),
        );
    }
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    run(&w, &host, "FixedUpdate", 1, 1);
    run(&w, &host, "FixedCoroutines", 1, 1);
    ok(
        &w,
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
    );
    let before = w.snapshot();
    let events = ok(&w, json!({"op":"events.read","clear":false}));
    assert!(!events.as_array().unwrap().is_empty());
    assert_eq!(w.snapshot(), before);
    let context = phase(&w, &host, "Contacts", 1, 1);
    assert_eq!(ok(&w, json!({"op":"events.read","clear":false})), events);
    reject_unchanged(&w, json!({"op":"events.read","clear":true}));
    reject_unchanged(&w, json!({"op":"events.read"}));
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Contacts","context":context}),
    );
    assert_eq!(ok(&w, json!({"op":"events.read","clear":true})), events);
    assert_eq!(ok(&w, json!({"op":"events.read","clear":false})), json!([]));
}

use super::*;
fn reject_unchanged(w: &World, request: Value) {
    let before = w.snapshot();
    assert_eq!(w.call(request)["ok"], false);
    assert_eq!(w.snapshot(), before);
}
fn tail(w: &World, host: &Value, frame: u64, tick: u64) {
    for name in ["Update", "UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
        run(w, host, name, frame, tick);
    }
    ok(w, json!({"op":"frame.end","host":host,"frame":frame}));
}
#[test]
fn multiple_fixed_ticks_keep_edges_until_solver_and_do_not_repeat_them() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    ok(
        &w,
        json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]}),
    );
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":25}}),
    );
    for tick in 1..=2 {
        let context = phase(&w, &host, "FixedUpdate", 1, tick);
        let input = ok(&w, json!({"op":"input.get","phase":"fixed"}));
        assert_eq!(input["projection_id"], tick);
        assert_eq!(input["keys"], json!(["W"]));
        assert_eq!(
            input["down"],
            if tick == 1 { json!(["W"]) } else { json!([]) }
        );
        ok(
            &w,
            json!({"op":"phase.claim","phase":"FixedUpdate","context":context}),
        );
        ok(
            &w,
            json!({"op":"phase.finish","host":host,"context":context}),
        );
        run(&w, &host, "FixedCoroutines", 1, tick);
        ok(
            &w,
            json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":tick}),
        );
        assert_eq!(ok(&w, json!({"op":"input.get","phase":"fixed"})), input);
        reject_unchanged(
            &w,
            json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":tick}),
        );
        run(&w, &host, "Contacts", 1, tick);
    }
    tail(&w, &host, 1, 2);
    let input = ok(&w, json!({"op":"input.get","phase":"fixed"}));
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
    );
    tail(&w, &host, 2, 2);
    assert_eq!(ok(&w, json!({"op":"input.get","phase":"fixed"})), input);
}
#[test]
fn native_schedule_rejects_phase_skips_stale_frames_invalid_time_and_unclaimed_finish() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    reject_unchanged(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    let context = phase(&w, &host, "Initialize", 0, 0);
    reject_unchanged(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    for dt in [
        json!({"num":1,"den":0}),
        json!({"num":2,"den":1}),
        json!({"num":1,"den":1000000000001u64}),
    ] {
        reject_unchanged(&w, json!({"op":"frame.open","host":host,"dt":dt}));
    }
    ok(
        &w,
        json!({"op":"session.configure","host":host,"fixed_delta_time":{"num":1,"den":2000}}),
    );
    reject_unchanged(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":1}}),
    );
    ok(
        &w,
        json!({"op":"session.configure","host":host,"fixed_delta_time":{"num":1,"den":50}}),
    );
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    for request in [
        json!({"op":"phase.open","host":host,"phase":"Update","frame":1,"fixed_tick":0}),
        json!({"op":"phase.open","host":host,"phase":"FixedUpdate","frame":0,"fixed_tick":1}),
        json!({"op":"phase.open","host":host,"phase":"FixedUpdate","frame":1,"fixed_tick":0}),
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
        json!({"op":"frame.end","host":host,"frame":1}),
    ] {
        reject_unchanged(&w, request);
    }
}
#[test]
fn normal_callback_transactions_are_atomic_and_strict_controls_cannot_be_staged() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    let create = json!({"op":"object.create","name":"rolled back"});
    for control in [
        json!({"op":"phase.finish","host":host,"context":context}),
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
        json!({"op":"session.close","host":host}),
    ] {
        reject_unchanged(
            &w,
            json!({"version":1,"op":"transaction","operations":[create,control]}),
        );
    }
    let request = json!({"version":1,"requestId":"rollback","op":"transaction","operations":[create,{"op":"object.destroy","object":0}]});
    reject_unchanged(&w, request.clone());
    let result = w.call(request.clone());
    assert_eq!(w.call(request), result);
    let result = ok(
        &w,
        json!({"version":1,"op":"transaction","operations":[create]}),
    );
    assert_eq!(result.as_array().unwrap().len(), 1);
}
#[test]
fn request_capacity_failure_does_not_consume_a_phase_claim() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    let mut responses = Vec::new();
    for _ in 0..1024 {
        let (code, response) = support::submit(w.0, br#"{"op":"time.get"}"#);
        assert_eq!(code, 0);
        responses.push(response);
    }
    let request =
        serde_json::to_vec(&json!({"op":"phase.claim","phase":"Initialize","context":context}))
            .unwrap();
    let (code, response) = support::submit(w.0, &request);
    assert_eq!(code, worldrunner_disunity_ffi::abi::result_code::BUDGET);
    assert_eq!(response, 0);
    for response in responses {
        assert_eq!(
            unsafe { worldrunner_disunity_ffi::abi::disunity_response_free(response) }.code,
            0
        );
    }
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
}
#[test]
fn copied_contexts_fail_across_strict_worlds_and_retired_generations() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let other = World::new();
    let host = open(&w);
    open(&other);
    let context = phase(&w, &host, "Initialize", 0, 0);
    reject_unchanged(
        &other,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    let old = w.0;
    drop(w);
    let replacement = World::new();
    assert_ne!(replacement.0, old);
    let new_host = open(&replacement);
    phase(&replacement, &new_host, "Initialize", 0, 0);
    reject_unchanged(
        &replacement,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    let bytes =
        serde_json::to_vec(&json!({"op":"phase.claim","phase":"Initialize","context":context}))
            .unwrap();
    assert_eq!(
        support::submit(old, &bytes).0,
        worldrunner_disunity_ffi::abi::result_code::INVALID_HANDLE
    );
}

#[test]
fn cancelled_contact_scope_restores_delta_projection() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":3,"den":100}}),
    );
    run(&w, &host, "FixedUpdate", 1, 1);
    run(&w, &host, "FixedCoroutines", 1, 1);
    ok(
        &w,
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
    );
    let before = ok(&w, json!({"op":"time.get"}));
    let context = phase(&w, &host, "Contacts", 1, 1);
    ok(
        &w,
        json!({"op":"phase.cancel","host":host,"context":context}),
    );
    assert_eq!(ok(&w, json!({"op":"time.get"})), before);
}

#[test]
fn strict_session_rejects_an_already_open_legacy_lifetime_without_clock_change() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    ok(&w, json!({"op":"lifetime.begin"}));
    reject_unchanged(
        &w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    ok(&w, json!({"op":"lifetime.end"}));
    open(&w);
}

#[test]
fn strict_callback_response_overflow_rolls_back_gameplay_rng_and_lease_state() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    let context = phase(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
    );
    let object = ok(&w, json!({"op":"object.create","name":"n".repeat(600_000)}))["handle"].clone();
    ok(&w, json!({"op":"random.seed","seed":19}));
    let draw = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    let expected = ok(&w, draw.clone());
    ok(&w, json!({"op":"random.seed","seed":19}));
    let before = w.snapshot();
    let mut operations = vec![
        json!({"op":"object.set","object":object,"active":false}),
        draw.clone(),
    ];
    operations.extend((0..30).map(|_| json!({"op":"object.get","object":object})));
    let request = json!({"version":1,"requestId":"strict-overflow","op":"transaction","operations":operations});
    let result = w.call(request.clone());
    assert_eq!(
        result["error"]["message"],
        "response budget exceeded; operation rolled back"
    );
    assert_eq!(w.snapshot(), before);
    assert_eq!(w.call(request), result);
    assert_eq!(ok(&w, draw), expected);
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
}

#[test]
fn fixed_callback_body_command_precedes_actual_solver_contacts() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    ok(&w, json!({"op":"physics.configure","gravity":[0,0]}));
    let moving = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let stationary = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let body = ok(&w, json!({"op":"component.add","object":moving,"kind":"Rigidbody2D","fields":{"body_type":"kinematic"}}))["handle"].clone();
    for object in [moving.clone(), stationary] {
        ok(
            &w,
            json!({"op":"component.add","object":object,"kind":"BoxCollider2D","fields":{"is_trigger":true}}),
        );
    }
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":50}}),
    );
    let context = phase(&w, &host, "FixedUpdate", 1, 1);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"FixedUpdate","context":context}),
    );
    assert_eq!(ok(&w, json!({"op":"events.read","clear":false})), json!([]));
    ok(
        &w,
        json!({"op":"body.move","component":body,"position":[0.25,0]}),
    );
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    run(&w, &host, "FixedCoroutines", 1, 1);
    ok(
        &w,
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
    );
    let context = phase(&w, &host, "Contacts", 1, 1);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Contacts","context":context}),
    );
    let events = ok(&w, json!({"op":"events.read"}));
    assert!(!events.as_array().unwrap().is_empty());
    let transform = ok(&w, json!({"op":"transform.get","object":moving}));
    assert!((transform["local_position"][0].as_f64().unwrap() - 0.25).abs() < 1e-6);
}

#[test]
fn contacts_use_committed_tick_fixed_delta_until_dynamic_update_opens() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let host = open(&w);
    run(&w, &host, "Initialize", 0, 0);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":3,"den":100}}),
    );
    run(&w, &host, "FixedUpdate", 1, 1);
    run(&w, &host, "FixedCoroutines", 1, 1);
    ok(
        &w,
        json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":1}),
    );
    let context = phase(&w, &host, "Contacts", 1, 1);
    let clock = ok(&w, json!({"op":"time.get"}));
    assert!((clock["delta_time"].as_f64().unwrap() - 0.02).abs() < 1e-6);
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Contacts","context":context}),
    );
    ok(
        &w,
        json!({"op":"phase.finish","host":host,"context":context}),
    );
    let context = phase(&w, &host, "Update", 1, 1);
    assert!(
        (ok(&w, json!({"op":"time.get"}))["delta_time"]
            .as_f64()
            .unwrap()
            - 0.03)
            .abs()
            < 1e-6
    );
    ok(
        &w,
        json!({"op":"phase.cancel","host":host,"context":context}),
    );
    assert_eq!(ok(&w, json!({"op":"time.get"})), clock);
}
