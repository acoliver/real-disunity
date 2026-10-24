use super::*;

fn rejected(w: &World, request: Value) {
    let before = w.snapshot();
    let status = ok(w, json!({"op":"session.status"}));
    assert_eq!(w.call(request)["ok"], false);
    assert_eq!(w.snapshot(), before);
    assert_eq!(ok(w, json!({"op":"session.status"})), status);
}
fn dispose(w: &World, c: &Value, routine: &Value) {
    let t = next(w, c);
    assert_eq!(t["kind"], "Dispose");
    assert_eq!(&t["routine"], routine);
    enter(w, c, &t);
    complete(w, c, &t, "ok");
}
fn suspended(w: &World, c: &Value, a: &Value) {
    complete(w, c, &a["immediate"], "more");
    let t = next(w, c);
    assert_eq!(t["kind"], "Current");
    assert_eq!(t["routine"], a["routine"]);
    enter(w, c, &t);
    complete(w, c, &t, "ok");
}

#[test]
fn depth_32_rejects_without_mutation_and_unwinds_lifo() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let mut scopes = Vec::new();
    for _ in 0..32 {
        let a = child(&w, &c, s);
        enter(&w, &c, &a["immediate"]);
        scopes.push(a);
    }
    rejected(
        &w,
        json!({"op":"readiness.start","context":c,"component":s}),
    );
    rejected(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":parent["ticket"],"outcome":"ok"}),
    );
    for a in scopes.into_iter().rev() {
        complete(&w, &c, &a["immediate"], "done");
        dispose(&w, &c, &a["routine"]);
        assert_eq!(
            w.call(json!({"op":"readiness.next","context":c}))["ok"],
            false
        );
    }
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn current_and_dispose_starts_reject_before_registration_or_revision_changes() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let a = child(&w, &c, s);
    enter(&w, &c, &a["immediate"]);
    complete(&w, &c, &a["immediate"], "more");
    let t = next(&w, &c);
    enter(&w, &c, &t);
    rejected(
        &w,
        json!({"op":"readiness.start","context":c,"component":s}),
    );
    complete(&w, &c, &t, "failed");
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    enter(&w, &c, &t);
    rejected(
        &w,
        json!({"op":"readiness.start","context":c,"component":s}),
    );
    complete(&w, &c, &t, "failed");
    rejected(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn active_ancestor_and_self_join_reject_then_valid_suspend_restores_parent() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let a = child(&w, &c, s);
    enter(&w, &c, &a["immediate"]);
    let b = child(&w, &c, s);
    enter(&w, &c, &b["immediate"]);
    suspended(&w, &c, &b);
    for target in [&a["routine"], &b["routine"]] {
        rejected(
            &w,
            json!({"op":"readiness.suspend","context":c,"routine":b["routine"],"wait":{"kind":"join","routine":target}}),
        );
    }
    for kind in ["predicate", "ienumerator"] {
        rejected(
            &w,
            json!({"op":"readiness.suspend","context":c,"routine":b["routine"],"wait":{"kind":kind}}),
        );
    }
    ok(
        &w,
        json!({"op":"readiness.suspend","context":c,"routine":b["routine"],"wait":null}),
    );
    complete(&w, &c, &a["immediate"], "done");
    dispose(&w, &c, &a["routine"]);
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn cancelled_child_selection_cannot_escape_to_peer_and_disposes_once() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let a = child(&w, &c, s);
    ok(
        &w,
        json!({"op":"readiness.cancel","context":c,"routine":a["routine"]}),
    );
    assert_eq!(
        ok(
            &w,
            json!({"op":"readiness.enter","context":c,"ticket":a["immediate"]["ticket"]})
        )["execute"],
        false
    );
    dispose(&w, &c, &a["routine"]);
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn response_admission_rolls_back_nested_registration_and_restores_parent() {
    use worldrunner_disunity_ffi::response::{MAX_RESPONSES, request_bytes, response_free};

    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let before = w.snapshot();
    let mut retained = Vec::new();
    for _ in 0..MAX_RESPONSES {
        retained.push(request_bytes(w.0, br#"{"op":"time.get"}"#).unwrap());
    }
    let request = serde_json::to_vec(&json!({
        "op":"readiness.start","context":c,"component":s
    }))
    .unwrap();
    let rejected = request_bytes(w.0, &request);
    for response in retained {
        response_free(response).unwrap();
    }
    assert_eq!(rejected, Err(2));
    assert_eq!(w.snapshot(), before);

    let child = child(&w, &c, s);
    assert_eq!(child["immediate"]["kind"], "MoveNext");
    enter(&w, &c, &child["immediate"]);
    complete(&w, &c, &child["immediate"], "done");
    dispose(&w, &c, &child["routine"]);
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}
#[test]
fn conservative_child_reservation_preserves_4096_root_claim_limit() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    for _ in 0..2047 {
        let a = child(&w, &c, s);
        enter(&w, &c, &a["immediate"]);
        complete(&w, &c, &a["immediate"], "done");
        dispose(&w, &c, &a["routine"]);
        ok(
            &w,
            json!({"op":"readiness.release","context":c,"routine":a["routine"]}),
        );
    }
    rejected(
        &w,
        json!({"op":"readiness.start","context":c,"component":s}),
    );
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn selection_requires_fresh_world_and_keeps_old_profile_single_ticket() {
    let w = World::new();
    ok(&w, json!({"op":"object.create"}));
    rejected(
        &w,
        json!({"op":"session.open","profile":"native_readiness_nested_start_v1"}),
    );
    let h = ok(
        &w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    assert_eq!(h["profile"], "native_phase_context_v1");
}
