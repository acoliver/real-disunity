use super::*;

#[test]
fn terminal_readiness_budget_exhaustion_keeps_truthful_obligations_until_close() {
    let w = World::new();
    let h = open(&w);
    let old = issue(&w, &h, "Initialize");
    claim(&w, &old);
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let s = ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    ok(
        &w,
        json!({"op":"readiness.register","context":old,"component":s,"callbacks":["OnDisable","OnDestroy"]}),
    );
    let enable = ok(&w, json!({"op":"readiness.next","context":old}));
    assert!(enable.is_null());
    for _ in 0..4096 {
        ok(
            &w,
            json!({"op":"readiness.start","context":old,"component":s}),
        );
    }
    let c = begin(&w, &h);
    claim(&w, &c);
    for _ in 0..4096 {
        let t = ok(&w, json!({"op":"readiness.next","context":c}));
        assert!(matches!(t["kind"].as_str(), Some("OnDisable" | "Dispose")));
        ok(
            &w,
            json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
        );
        ok(
            &w,
            json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"failed"}),
        );
    }
    let error = w.call(json!({"op":"readiness.next","context":c}));
    assert_eq!(error["ok"], false);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("budget")
    );
    reject(&w, json!({"op":"phase.finish","host":h,"context":c}));
    let s = ok(&w, json!({"op":"session.status"}));
    let obligations = &s["terminal_recovery"]["native_obligations"];
    assert_eq!(obligations["claims"], 4096);
    assert_eq!(obligations["remaining_routines"], 1);
    assert_eq!(obligations["remaining_scripts"], 1);
    let closed = ok(&w, json!({"op":"session.close","host":h}));
    assert_eq!(closed["stage"], "Closed");
    assert_eq!(
        closed["terminal_recovery"]["native_obligations"],
        *obligations
    );
}

#[test]
fn recovery_gets_a_fresh_readiness_budget_after_exhausted_gameplay_tickets() {
    let w = World::new();
    let h = open(&w);
    let old = issue(&w, &h, "Initialize");
    claim(&w, &old);
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let s = ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    ok(
        &w,
        json!({"op":"readiness.register","context":old,"component":s,"callbacks":["OnEnable","OnDisable","OnDestroy"]}),
    );
    for i in 0..4095 {
        if i > 0 {
            ok(
                &w,
                json!({"op":"component.set","component":s,"enabled":i%2==0}),
            );
        }
        let t = ok(&w, json!({"op":"readiness.next","context":old}));
        ok(
            &w,
            json!({"op":"readiness.enter","context":old,"ticket":t["ticket"]}),
        );
        ok(
            &w,
            json!({"op":"readiness.complete","context":old,"ticket":t["ticket"],"outcome":"ok"}),
        );
    }
    let c = begin(&w, &h);
    claim(&w, &c);
    for kind in ["OnDisable", "OnDestroy"] {
        let t = ok(&w, json!({"op":"readiness.next","context":c}));
        assert_eq!(t["kind"], kind);
        ok(
            &w,
            json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
        );
        ok(
            &w,
            json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"failed"}),
        );
    }
    finish(&w, &h, &c);
}
