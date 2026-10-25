use super::*;
use worldrunner_disunity_ffi::{request_value, response::*};

#[test]
fn terminal_authentication_linear_admission_and_permanent_close() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let other = World::new();
    let h = open(&w);
    let foreign = open(&other);
    let request = json!({"op":"session.begin_cleanup","host":h});
    for bad in [
        json!({"op":"session.begin_cleanup","host":foreign}),
        json!({"op":"session.begin_cleanup","host":{"session":h["session"],"host_key":"bad"}}),
        json!({"op":"session.begin_cleanup","host":h,"reason":"arbitrary secret text"}),
        json!({"op":"session.begin_cleanup","host":h,"extra":1}),
        json!({"version":1,"requestId":"fatal","op":"session.begin_cleanup","host":h}),
        json!({"version":1,"op":"transaction","operations":[request]}),
    ] {
        reject(&w, bad);
    }
    let handle = w.0;
    let bytes = serde_json::to_vec(&request).unwrap();
    let result = std::thread::spawn(move || support::call(handle, &bytes))
        .join()
        .unwrap();
    assert_eq!(result["ok"], false);
    let c = begin(&w, &h);
    reject(&w, request.clone());
    reject_forged_contexts(&w, &c);
    reject(
        &other,
        json!({"op":"phase.claim","phase":"Cleanup","context":c}),
    );
    reject(&w, json!({"op":"object.create"}));
    claim(&w, &c);
    reject(
        &w,
        json!({"op":"phase.claim","phase":"Cleanup","context":c}),
    );
    let text = format!("{}{}", w.snapshot(), ok(&w, json!({"op":"session.status"})));
    assert!(!text.contains(h["host_key"].as_str().unwrap()));
    assert!(!text.contains(c["token"].as_str().unwrap()));
    finish(&w, &h, &c);
    reject(&w, request);
    reject(
        &w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
}

#[test]
fn terminal_rejects_claimed_cleanup_but_takes_over_unclaimed_cleanup() {
    let _table = crate::response_table::exclusive();
    for claimed in [false, true] {
        let w = World::new();
        let h = open(&w);
        let old = issue(&w, &h, "Cleanup");
        if claimed {
            claim(&w, &old);
            reject(&w, json!({"op":"session.begin_cleanup","host":h}));
            finish(&w, &h, &old);
        } else {
            let c = begin(&w, &h);
            reject(
                &w,
                json!({"op":"phase.claim","phase":"Cleanup","context":old}),
            );
            claim(&w, &c);
            finish(&w, &h, &c);
        }
    }
    let legacy = World::new();
    reject(
        &legacy,
        json!({"op":"session.begin_cleanup","host":{"session":"none","host_key":"none"}}),
    );
}

#[test]
fn terminal_only_writes_and_transactions_cannot_reopen_gameplay() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let h = open(&w);
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let c = begin(&w, &h);
    claim(&w, &c);
    for request in [
        json!({"op":"component.add","object":o,"kind":"Script"}),
        json!({"op":"object.clone","object":o}),
        json!({"op":"readiness.register","context":c,"component":o,"callbacks":[]}),
        json!({"op":"readiness.start","context":c,"component":o}),
        json!({"op":"input.set","keys":["W"]}),
        json!({"op":"input.focus","focused":false}),
        json!({"op":"input.configure_axes","axes":{}}),
        json!({"op":"physics.configure","gravity":[0,0]}),
        json!({"op":"physics.step"}),
        json!({"op":"frame.open","host":h,"dt":{"num":0,"den":1}}),
        json!({"op":"session.configure","host":h,"time_scale":{"num":0,"den":1}}),
    ] {
        reject(&w, request.clone());
        reject(
            &w,
            json!({"version":1,"op":"transaction","operations":[{"op":"object.set","object":o,"name":"rolled back"},request]}),
        );
    }
    let s = ok(&w, json!({"op":"session.status"}));
    ok(
        &w,
        json!({"version":1,"op":"transaction","operations":[{"op":"object.set","object":o,"name":"cleanup"},{"op":"component.add","object":o,"kind":"SpriteRenderer"}]}),
    );
    assert_eq!(
        ok(&w, json!({"op":"session.status"}))["content_revision"]
            .as_u64()
            .unwrap(),
        s["content_revision"].as_u64().unwrap() + 1
    );
    finish(&w, &h, &c);
}

/// Fills the world's outstanding reply bytes with descending reply sizes until even a
/// `time.get` reply no longer fits, well before the response handle limit.
fn fill_outstanding_bytes(w: &World, objects: &[Value]) -> Vec<u64> {
    let requests = objects
        .iter()
        .map(|object| json!({"op":"object.get","object":object}))
        .chain([json!({"op":"time.get"})]);
    let mut held = Vec::new();
    for request in requests {
        let bytes = serde_json::to_vec(&request).unwrap();
        while let Ok(response) = request_bytes(w.0, &bytes) {
            held.push(response);
        }
    }
    assert!(held.len() < MAX_RESPONSES, "the byte limit must bind first");
    held
}

#[test]
fn terminal_response_admission_is_atomic_at_handle_and_byte_limits() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let h = open(&w);
    let objects = [900_000, 90_000, 9_000, 900, 90].map(|length| {
        ok(&w, json!({"op":"object.create","name":"n".repeat(length)}))["handle"].clone()
    });
    let old = issue(&w, &h, "Initialize");
    claim(&w, &old);
    let before = ok(&w, json!({"op":"session.status"}));
    let request = serde_json::to_vec(&json!({"op":"session.begin_cleanup","host":h})).unwrap();
    let mut held = Vec::new();
    for _ in 0..MAX_RESPONSES {
        held.push(request_bytes(w.0, br#"{"op":"time.get"}"#).unwrap());
    }
    assert_eq!(request_bytes(w.0, &request), Err(2));
    assert_eq!(
        request_value(w.0, json!({"op":"session.status"})).unwrap()["data"],
        before
    );
    for r in held.drain(..) {
        response_free(r).unwrap();
    }
    held = fill_outstanding_bytes(&w, &objects);
    assert_eq!(request_bytes(w.0, &request), Err(2));
    assert_eq!(
        request_value(w.0, json!({"op":"session.status"})).unwrap()["data"],
        before
    );
    for r in held {
        response_free(r).unwrap();
    }
    let c = begin(&w, &h);
    claim(&w, &c);
    finish(&w, &h, &c);
}

#[test]
fn terminal_transition_stales_current_snapshot_and_preserves_historical_bytes() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let h = open(&w);
    run(&w, &h, "Initialize");
    let capture = ok(&w, json!({"op":"snapshot.capture","host":h}));
    let s = ok(&w, json!({"op":"session.status"}));
    let c = begin(&w, &h);
    let after = ok(&w, json!({"op":"session.status"}));
    assert_eq!(
        after["content_revision"].as_u64().unwrap(),
        s["content_revision"].as_u64().unwrap() + 1
    );
    reject(
        &w,
        json!({"op":"snapshot.validate","host":h,"stamp":capture["stamp"],"require_current":true}),
    );
    assert_eq!(
        ok(
            &w,
            json!({"op":"snapshot.validate","host":h,"stamp":capture["stamp"],"require_current":false})
        )["current"],
        false
    );
    assert_eq!(capture["stamp"]["content_revision"], s["content_revision"]);
    claim(&w, &c);
    finish(&w, &h, &c);
}

fn reject_forged_contexts(w: &World, c: &Value) {
    for key in [
        "world",
        "frame",
        "fixed_tick",
        "sequence",
        "session",
        "phase",
        "token",
    ] {
        let mut bad = c.clone();
        bad[key] = if matches!(key, "session" | "phase" | "token") {
            json!("forged")
        } else {
            json!(u64::MAX)
        };
        reject(
            w,
            json!({"op":"phase.claim","phase":"Cleanup","context":bad}),
        );
    }
}
