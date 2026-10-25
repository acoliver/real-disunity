#[path = "cleanup_admission.rs"]
mod admission;
use super::*;

fn setup(w: &World) -> (Value, Value, Vec<u64>) {
    let (host, c) = open(w);
    let components = (0..2)
        .map(|_| script(w, &c, json!(["OnDisable", "OnDestroy"])).1)
        .collect();
    assert!(next(w, &c).is_null());
    finish(w, &host, &c);
    let c = claim(w, &host, "Cleanup", 0, 0);
    (host, c, components)
}

#[test]
fn cleanup_mutation_before_peer_preserves_failed_writes_and_once_only_attempts() {
    let w = World::new();
    let (host, c, components) = setup(&w);
    let t = next(&w, &c);
    assert_eq!(t["component"], components[0]);
    assert_eq!(t["kind"], "OnDisable");
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(
        &w,
        json!({"op":"component.remove","component":components[1]}),
    );
    ok(
        &w,
        json!({"op":"component.set","component":components[0],"enabled":false}),
    );
    let done =
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"failed"});
    ok(&w, done.clone());
    ok(&w, done);
    assert_eq!(
        w.call(json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}))["ok"],
        false
    );
    for (component, kind) in [
        (components[1], "OnDisable"),
        (components[0], "OnDestroy"),
        (components[1], "OnDestroy"),
    ] {
        let t = next(&w, &c);
        assert_eq!(t["component"], component);
        assert_eq!(t["kind"], kind);
        complete(&w, &c, &t, "ok");
    }
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
    assert_eq!(ok(&w, json!({"op":"session.status"}))["stage"], "Closed");
}

#[test]
fn cleanup_unknown_binding_and_wrong_context_leave_current_ticket_available() {
    let w = World::new();
    let (host, c, components) = setup(&w);
    let t = next(&w, &c);
    for request in [
        json!({"op":"readiness.register","context":c,"component":u64::MAX,"callbacks":["OnDestroy"]}),
        json!({"op":"readiness.start","context":c,"component":u64::MAX}),
        json!({"op":"readiness.enter","context":c,"ticket":u64::MAX}),
    ] {
        assert_eq!(w.call(request)["ok"], false);
    }
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
        bad[key] = if ["session", "token"].contains(&key) {
            json!("forged")
        } else if key == "phase" {
            json!("Initialize")
        } else {
            json!(999)
        };
        assert_eq!(
            w.call(json!({"op":"readiness.enter","context":bad,"ticket":t["ticket"]}))["ok"],
            false
        );
    }
    assert_eq!(t["component"], components[0]);
    complete(&w, &c, &t, "ok");
    while let t = next(&w, &c)
        && !t.is_null()
    {
        complete(&w, &c, &t, "ok");
    }
    finish(&w, &host, &c);
}

fn reject_at_handle_limit(w: &World, request: Value) {
    use worldrunner_disunity_ffi::response::{MAX_RESPONSES, request_bytes, response_free};
    let before = ok(w, json!({"op":"session.status"}));
    let held: Vec<_> = (0..MAX_RESPONSES)
        .map(|_| request_bytes(w.0, br#"{"op":"time.get"}"#).unwrap())
        .collect();
    assert_eq!(
        request_bytes(w.0, &serde_json::to_vec(&request).unwrap()),
        Err(2)
    );
    for handle in held {
        response_free(handle).unwrap();
    }
    assert_eq!(ok(w, json!({"op":"session.status"})), before);
}

#[test]
fn cleanup_response_admission_rolls_back_selection_entry_completion_and_close() {
    let w = World::new();
    let (host, c, _) = setup(&w);
    reject_at_handle_limit(&w, json!({"op":"readiness.next","context":c}));
    for _ in 0..4 {
        let t = next(&w, &c);
        let enter = json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]});
        reject_at_handle_limit(&w, enter.clone());
        assert_eq!(ok(&w, enter)["execute"], true);
        let done =
            json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"});
        reject_at_handle_limit(&w, done.clone());
        ok(&w, done);
    }
    assert!(next(&w, &c).is_null());
    reject_at_handle_limit(&w, json!({"op":"phase.finish","host":host,"context":c}));
    finish(&w, &host, &c);
}
