use super::*;

#[test]
fn every_stamp_field_is_exact_and_unsigned() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    let s = &a["stamp"];
    for (key, original) in s.as_object().unwrap() {
        let mut forged = s.clone();
        forged[key] = if original.is_string() {
            json!("forged")
        } else {
            json!(u64::MAX)
        };
        assert_eq!(valid(&w, &h, &forged, false)["ok"], false, "{key}");
        let mut missing = s.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert_eq!(valid(&w, &h, &missing, false)["ok"], false, "{key}");
        if original.is_number() {
            for value in [json!(-1), json!(1.0), json!("1"), json!(null)] {
                forged[key] = value;
                assert_eq!(valid(&w, &h, &forged, false)["ok"], false, "{key}");
            }
        }
    }
    let mut unknown = s.clone();
    unknown["extra"] = json!(true);
    assert_eq!(valid(&w, &h, &unknown, false)["ok"], false);
    assert_eq!(valid(&w, &h, s, true)["ok"], true);
}
#[test]
fn private_host_and_actual_world_session_are_required() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    let other = World::new();
    let (oh, oc) = setup(&other);
    idle(&other, &oh, &oc);
    for host in [
        json!({"session":h["session"],"host_key":"wrong"}),
        oh.clone(),
        a["stamp"].clone(),
    ] {
        assert_eq!(valid(&w, &host, &a["stamp"], false)["ok"], false);
        assert_eq!(
            w.call(json!({"op":"snapshot.capture","host":host}))["ok"],
            false
        );
    }
    assert_eq!(valid(&other, &oh, &a["stamp"], false)["ok"], false);
    let text = a.to_string();
    assert!(!text.contains(h["host_key"].as_str().unwrap()));
    assert!(!text.contains(c["token"].as_str().unwrap()));
    ok(&w, json!({"op":"session.close","host":h}));
    assert_eq!(valid(&w, &h, &a["stamp"], false)["ok"], false);
}
#[test]
fn capture_rejects_uninitialized_active_commit_and_closed_stages() {
    let w = World::new();
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":{"session":"none","host_key":"none"}}))["ok"],
        false
    );
    let h = ok(
        &w,
        json!({"op":"session.open","profile":"native_phase_context_v1"}),
    );
    let h = json!({"session":h["session"],"host_key":h["host_key"]});
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":h}))["ok"],
        false
    );
    let c = claim(&w, &h, "Initialize", 0, 0);
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":h}))["ok"],
        false
    );
    idle(&w, &h, &c);
    capture(&w, &h);
    ok(
        &w,
        json!({"op":"frame.open","host":h,"dt":{"num":1,"den":50}}),
    );
    for phase in ["FixedUpdate", "FixedCoroutines"] {
        let c = claim(&w, &h, phase, 1, 1);
        idle(&w, &h, &c);
    }
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":h}))["ok"],
        false
    );
    ok(&w, json!({"op":"session.close","host":h}));
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":h}))["ok"],
        false
    );
}
pub(super) fn claim(w: &World, h: &Value, phase: &str, frame: u64, tick: u64) -> Value {
    let c = ok(
        w,
        json!({"op":"phase.open","host":h,"phase":phase,"frame":frame,"fixed_tick":tick}),
    )["context"]
        .clone();
    ok(w, json!({"op":"phase.claim","phase":phase,"context":c}));
    c
}
#[test]
fn pending_readiness_ticket_blocks_capture_and_finish() {
    let w = World::new();
    let (h, c) = setup(&w);
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let s = ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    ok(
        &w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":["Awake"]}),
    );
    let t = ok(&w, json!({"op":"readiness.next","context":c}));
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":h}))["ok"],
        false
    );
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":h,"context":c}))["ok"],
        false
    );
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"}),
    );
    idle(&w, &h, &c);
    capture(&w, &h);
}
#[test]
fn receipt_requests_reject_ids_transactions_missing_and_unknown_fields() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    for request in [
        json!({"version":1,"requestId":"no","op":"snapshot.capture","host":h}),
        json!({"op":"snapshot.capture","host":h,"extra":true}),
        json!({"op":"snapshot.validate","host":h,"stamp":a["stamp"]}),
        json!({"op":"snapshot.validate","stamp":a["stamp"],"require_current":false}),
        json!({"version":1,"op":"transaction","operations":[{"op":"snapshot.release","host":h,"stamp":a["stamp"]}]}),
    ] {
        assert_eq!(w.call(request)["ok"], false);
    }
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
}
#[test]
fn snapshot_and_gameplay_authority_are_owner_thread_only() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    let before = revision(&w);
    for request in [
        json!({"op":"snapshot.capture","host":h}),
        json!({"op":"snapshot.validate","host":h,"stamp":a["stamp"],"require_current":false}),
        json!({"op":"snapshot.release","host":h,"stamp":a["stamp"]}),
        json!({"op":"input.events","events":[]}),
    ] {
        let world = w.0;
        let result = std::thread::spawn(move || {
            support::call(world, &serde_json::to_vec(&request).unwrap())
        })
        .join()
        .unwrap();
        assert_eq!(result["ok"], false);
    }
    assert_eq!(revision(&w), before);
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
}
