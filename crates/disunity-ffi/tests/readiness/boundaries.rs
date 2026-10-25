use super::*;

#[test]
fn fixed_waits_use_admitted_cutoff_and_end_frame_has_its_own_lane() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    let a = start(&w, &c, s);
    suspend(&w, &c, a, json!({"kind":"fixed"}));
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":25}}),
    );
    for tick in 1..=2 {
        let c = claim(&w, &host, "FixedUpdate", 1, tick);
        assert!(next(&w, &c).is_null());
        finish(&w, &host, &c);
        let c = claim(&w, &host, "FixedCoroutines", 1, tick);
        suspend(&w, &c, a, json!({"kind":"fixed"}));
        assert!(next(&w, &c).is_null());
        finish(&w, &host, &c);
        ok(
            &w,
            json!({"op":"fixed.commit","host":host,"frame":1,"fixed_tick":tick}),
        );
        let c = claim(&w, &host, "Contacts", 1, tick);
        finish(&w, &host, &c);
    }
    for p in ["Update", "UpdateCoroutines"] {
        let c = claim(&w, &host, p, 1, 2);
        assert!(next(&w, &c).is_null());
        finish(&w, &host, &c);
    }
    let c = claim(&w, &host, "LateUpdate", 1, 2);
    let b = start(&w, &c, s);
    suspend(&w, &c, b, json!({"kind":"end_frame"}));
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
    let c = claim(&w, &host, "EndOfFrame", 1, 2);
    suspend(&w, &c, b, json!({"kind":"end_frame"}));
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
    ok(&w, json!({"op":"frame.end","host":host,"frame":1}));
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
    );
    for p in ["Update", "UpdateCoroutines", "LateUpdate"] {
        let c = claim(&w, &host, p, 2, 2);
        assert!(next(&w, &c).is_null());
        finish(&w, &host, &c);
    }
    let c = claim(&w, &host, "EndOfFrame", 2, 2);
    let t = next(&w, &c);
    assert_eq!(t["routine"], b);
    complete(&w, &c, &t, "done");
    let t = next(&w, &c);
    complete(&w, &c, &t, "ok");
    assert!(next(&w, &c).is_null());
}
#[test]
fn script_generations_kind_registration_and_cross_world_routines_are_checked() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (_, c) = open(&w);
    let (o, s) = script(&w, &c, json!([]));
    let transform =
        ok(&w, json!({"op":"object.get","object":o}))["components"][0]["handle"].clone();
    for component in [json!(0), transform] {
        assert_eq!(
            w.call(
                json!({"op":"readiness.register","context":c,"component":component,"callbacks":[]})
            )["ok"],
            false
        );
    }
    let other = World::new();
    let (_, oc) = open(&other);
    let (_, os) = script(&other, &oc, json!([]));
    let foreign = start(&other, &oc, os);
    assert_eq!(
        w.call(json!({"op":"readiness.cancel","context":c,"routine":foreign}))["ok"],
        false
    );
    assert_eq!(
        other.call(json!({"op":"readiness.next","context":c}))["ok"],
        false
    );
    ok(&w, json!({"op":"component.remove","component":s}));
    assert_eq!(
        w.call(json!({"op":"readiness.register","context":c,"component":s,"callbacks":[]}))["ok"],
        false
    );
    let new = ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    assert_ne!(new, s);
    assert_eq!(
        w.call(json!({"op":"readiness.start","context":c,"component":s}))["ok"],
        false
    );
    ok(
        &w,
        json!({"op":"readiness.register","context":c,"component":new,"callbacks":["Awake"]}),
    );
    let t = next(&w, &c);
    assert_eq!(t["component"], new);
    complete(&w, &c, &t, "ok");
}
#[test]
fn disabled_component_retains_routine_but_ancestor_inactivity_cancels_it() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (_, c) = open(&w);
    let (o, s) = script(&w, &c, json!([]));
    let r = start(&w, &c, s);
    ok(
        &w,
        json!({"op":"component.set","component":s,"enabled":false}),
    );
    let t = next(&w, &c);
    assert_eq!(t["kind"], "MoveNext");
    complete(&w, &c, &t, "more");
    let parent = ok(&w, json!({"op":"object.create","active":false}))["handle"].clone();
    ok(
        &w,
        json!({"op":"transform.parent","object":o,"parent":parent}),
    );
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    complete(&w, &c, &t, "failed");
    assert!(next(&w, &c).is_null());
    assert_eq!(
        w.call(json!({"op":"readiness.start","context":c,"component":s}))["ok"],
        false
    );
    ok(
        &w,
        json!({"op":"readiness.release","context":c,"routine":r}),
    );
}
#[test]
fn native_readiness_owner_affinity_and_private_context_redaction() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (_, c) = open(&w);
    script(&w, &c, json!(["Awake"]));
    let t = next(&w, &c);
    let handle = w.0;
    let ctx = c.clone();
    let ticket = t["ticket"].clone();
    let before = w.snapshot();
    let result = std::thread::spawn(move || {
        let request = json!({"op":"readiness.enter","context":ctx,"ticket":ticket});
        support::call(handle, &serde_json::to_vec(&request).unwrap())
    })
    .join()
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(w.snapshot(), before);
    assert!(!before.to_string().contains(c["token"].as_str().unwrap()));
    complete(&w, &c, &t, "ok");
}
#[test]
fn callback_created_script_start_and_missing_methods_are_once_only() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!(["Start", "Update"]));
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
    );
    let c = claim(&w, &host, "Update", 1, 0);
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Start");
    assert_eq!(t["component"], s);
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    let (_, new) = script(&w, &c, json!(["Start", "Update"]));
    ok(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"failed"}),
    );
    for (kind, owner) in [("Start", new), ("Update", s), ("Update", new)] {
        let t = next(&w, &c);
        assert_eq!(t["kind"], kind);
        assert_eq!(t["component"], owner);
        complete(&w, &c, &t, "ok");
    }
    assert!(next(&w, &c).is_null());
}
#[test]
fn bounded_claim_failure_preserves_native_flags_rng_time_and_ticket() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!(["OnEnable", "OnDisable"]));
    let t = next(&w, &c);
    complete(&w, &c, &t, "ok"); // Missing Awake also consumes one claim.
    for i in 0..4094 {
        ok(
            &w,
            json!({"op":"component.set","component":s,"enabled":i%2!=0}),
        );
        let t = next(&w, &c);
        complete(&w, &c, &t, "ok");
    }
    ok(
        &w,
        json!({"op":"component.set","component":s,"enabled":false}),
    );
    ok(&w, json!({"op":"random.seed","seed":42}));
    let before = w.snapshot();
    let r = w.call(json!({"op":"readiness.next","context":c}));
    assert_eq!(r["ok"], false);
    assert!(r["error"]["message"].as_str().unwrap().contains("budget"));
    assert_eq!(w.snapshot(), before);
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":0,"den":1}}),
    );
    let c = claim(&w, &host, "Update", 1, 0);
    let t = next(&w, &c);
    assert_eq!(t["kind"], "OnDisable");
    complete(&w, &c, &t, "ok");
}
#[test]
fn native_only_capability_is_independent_and_bounded() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let d = ok(&w, json!({"op":"describe"}));
    let caps = d["capabilities"].as_array().unwrap();
    assert!(caps.contains(&json!("native_readiness_primitives_v1")));
    assert!(!caps.contains(&json!("native_scheduler_v1")));
    assert!(!caps.contains(&json!("phase_tokens")));
    assert_eq!(d["native_readiness"]["max_claims_per_root"], 4096);
    assert_eq!(d["native_readiness"]["managed_integration"], false);
}
