use super::*;

fn add(w: &World, c: &Value, owner: &Value, callbacks: Value) -> Value {
    let s = ok(
        w,
        json!({"op":"component.add","object":owner,"kind":"Script"}),
    )["handle"]
        .clone();
    ok(
        w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":callbacks}),
    );
    s
}
fn object(w: &World) -> Value {
    ok(w, json!({"op":"object.create"}))["handle"].clone()
}
fn attempt(w: &World, c: &Value, component: &Value, kind: &str, outcome: &str) {
    let t = next(w, c);
    assert_eq!(t["component"], *component);
    assert_eq!(t["kind"], kind);
    assert!(t["routine"].is_null());
    enter(w, c, &t);
    complete(w, c, &t, outcome);
}
fn entered_start() -> (World, Value, Value) {
    let w = World::new();
    let h = ok(
        &w,
        json!({"op":"session.open","profile":"native_readiness_nested_start_v1"}),
    );
    let host = json!({"session":h["session"],"host_key":h["host_key"]});
    let c = claim(&w, &host, "Initialize", 0);
    add(&w, &c, &object(&w), json!(["Start"]));
    assert!(next(&w, &c).is_null());
    ok(&w, json!({"op":"phase.finish","host":host,"context":c}));
    ok(
        &w,
        json!({"op":"frame.open","host":host,"dt":{"num":1,"den":100}}),
    );
    let c = claim(&w, &host, "Update", 1);
    let parent = next(&w, &c);
    assert_eq!(parent["kind"], "Start");
    enter(&w, &c, &parent);
    (w, c, parent)
}
fn claim(w: &World, host: &Value, phase: &str, frame: u64) -> Value {
    let c = ok(
        w,
        json!({"op":"phase.open","host":host,"phase":phase,"frame":frame,"fixed_tick":0}),
    )["context"]
        .clone();
    ok(w, json!({"op":"phase.claim","phase":phase,"context":c}));
    c
}

#[test]
fn clone_lifecycle_is_synchronous_inside_start_and_restores_parent() {
    let (w, c, parent) = entered_start();
    let s = add(&w, &c, &object(&w), json!(["Awake", "OnEnable", "Start"]));
    attempt(&w, &c, &s, "Awake", "ok");
    attempt(&w, &c, &s, "OnEnable", "ok");
    assert!(next(&w, &c).is_null(), "Start must never execute nested");
    assert_eq!(
        w.call(json!({"op":"readiness.enter","context":c,"ticket":parent["ticket"]}))["ok"],
        false
    );
    complete(&w, &c, &parent, "ok");
    attempt(&w, &c, &s, "Start", "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn hierarchy_awakes_precede_enables_in_native_hierarchy_order() {
    let (w, c, parent) = entered_start();
    let leaf = object(&w);
    let root = object(&w);
    ok(
        &w,
        json!({"op":"transform.parent","object":leaf,"parent":root,"world_position_stays":false}),
    );
    let source_leaf = ok(
        &w,
        json!({"op":"component.add","object":leaf,"kind":"Script"}),
    )["handle"]
        .clone();
    let source_root = ok(
        &w,
        json!({"op":"component.add","object":root,"kind":"Script"}),
    )["handle"]
        .clone();
    let clone = ok(&w, json!({"op":"object.clone","object":root}));
    let a = clone["remap"][source_leaf.to_string()].clone();
    let b = clone["remap"][source_root.to_string()].clone();
    for component in [&a, &b] {
        ok(
            &w,
            json!({"op":"readiness.register","context":c,"component":component,"callbacks":["Awake","OnEnable"]}),
        );
    }
    for kind in ["Awake", "OnEnable"] {
        for s in [&b, &a] {
            attempt(&w, &c, s, kind, "ok");
        }
    }
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "ok");
}

#[test]
fn failed_clone_awake_is_consumed_and_parent_continues() {
    let (w, c, parent) = entered_start();
    let a = add(&w, &c, &object(&w), json!(["Awake", "OnEnable"]));
    let b = add(&w, &c, &object(&w), json!(["Awake", "OnEnable"]));
    attempt(&w, &c, &a, "Awake", "failed");
    attempt(&w, &c, &b, "Awake", "ok");
    for s in [&a, &b] {
        attempt(&w, &c, s, "OnEnable", "ok");
    }
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn unrelated_ready_routine_is_not_selected_by_reentrant_next() {
    let (w, c, s) = setup();
    let queued = child(&w, &c, s);
    assert!(queued.get("immediate").is_none());
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let clone = add(&w, &c, &object(&w), json!(["Awake"]));
    attempt(&w, &c, &clone, "Awake", "ok");
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "ok");
    let t = next(&w, &c);
    assert_eq!(t["routine"], queued["routine"]);
    assert_eq!(t["kind"], "MoveNext");
}

#[test]
fn lifecycle_depth_budget_rejects_registration_and_restores_every_ancestor() {
    let (w, c, _) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    let mut entered = vec![parent];
    for _ in 0..32 {
        let s = add(&w, &c, &object(&w), json!(["Awake"]));
        let t = next(&w, &c);
        assert_eq!(t["component"], s);
        enter(&w, &c, &t);
        entered.push(t);
    }
    let component = ok(
        &w,
        json!({"op":"component.add","object":object(&w),"kind":"Script"}),
    )["handle"]
        .clone();
    let before = w.snapshot();
    let rejected = w.call(
        json!({"op":"readiness.register","context":c,"component":component,"callbacks":["Awake"]}),
    );
    assert_eq!(rejected["ok"], false);
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("depth budget")
    );
    assert_eq!(before, w.snapshot());
    for t in entered.into_iter().rev() {
        complete(&w, &c, &t, "ok");
    }
    assert!(next(&w, &c).is_null());
}

#[test]
fn lifecycle_claim_budget_rejects_registration_without_losing_parent() {
    let (w, c, s) = setup();
    let parent = next(&w, &c);
    enter(&w, &c, &parent);
    for _ in 0..2047 {
        let child = child(&w, &c, s);
        enter(&w, &c, &child["immediate"]);
        complete(&w, &c, &child["immediate"], "done");
        let t = next(&w, &c);
        assert_eq!(t["kind"], "Dispose");
        enter(&w, &c, &t);
        complete(&w, &c, &t, "ok");
        ok(
            &w,
            json!({"op":"readiness.release","context":c,"routine":child["routine"]}),
        );
    }
    let component = ok(
        &w,
        json!({"op":"component.add","object":object(&w),"kind":"Script"}),
    )["handle"]
        .clone();
    let before = w.snapshot();
    let rejected = w.call(
        json!({"op":"readiness.register","context":c,"component":component,"callbacks":["Awake"]}),
    );
    assert_eq!(rejected["ok"], false);
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("claim budget")
    );
    assert_eq!(before, w.snapshot());
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn cloned_scripts_inside_movenext_do_not_run_a_queued_peer() {
    let (w, c, s) = setup();
    let awake = next(&w, &c);
    enter(&w, &c, &awake);
    complete(&w, &c, &awake, "ok");
    let routine = child(&w, &c, s);
    let parent = next(&w, &c);
    assert_eq!(parent["kind"], "MoveNext");
    enter(&w, &c, &parent);
    let peer = object(&w);
    ok(&w, json!({"op":"object.set","object":peer,"active":false}));
    let queued = add(&w, &c, &peer, json!(["Awake"]));
    ok(&w, json!({"op":"object.set","object":peer,"active":true}));
    let clone = add(&w, &c, &object(&w), json!(["Awake", "OnEnable"]));
    attempt(&w, &c, &clone, "Awake", "ok");
    attempt(&w, &c, &clone, "OnEnable", "ok");
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "done");
    attempt(&w, &c, &queued, "Awake", "ok");
    let t = next(&w, &c);
    assert_eq!(t["routine"], routine["routine"]);
    assert_eq!(t["kind"], "Dispose");
}

#[test]
fn invalidated_child_entry_restores_parent_and_missing_awake_is_native_consumed() {
    let (w, c, parent) = entered_start();
    let owner = object(&w);
    let s = add(&w, &c, &owner, json!(["OnEnable"]));
    let t = next(&w, &c);
    assert_eq!(t["kind"], "OnEnable");
    assert_eq!(t["component"], s);
    ok(&w, json!({"op":"object.set","object":owner,"active":false}));
    assert_eq!(
        ok(
            &w,
            json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]})
        )["execute"],
        false
    );
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn disabled_clone_awakes_without_enabling_and_inactive_clone_stays_deferred() {
    let (w, c, parent) = entered_start();
    let active = object(&w);
    let disabled = ok(
        &w,
        json!({"op":"component.add","object":active,"kind":"Script"}),
    )["handle"]
        .clone();
    ok(
        &w,
        json!({"op":"component.set","component":disabled,"enabled":false}),
    );
    ok(
        &w,
        json!({"op":"readiness.register","context":c,"component":disabled,"callbacks":["Awake","OnEnable"]}),
    );
    let inactive = object(&w);
    ok(
        &w,
        json!({"op":"object.set","object":inactive,"active":false}),
    );
    let deferred = add(&w, &c, &inactive, json!(["Awake", "OnEnable"]));
    attempt(&w, &c, &disabled, "Awake", "ok");
    assert!(next(&w, &c).is_null());
    ok(
        &w,
        json!({"op":"component.set","component":disabled,"enabled":true}),
    );
    attempt(&w, &c, &disabled, "OnEnable", "ok");
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "ok");
    ok(
        &w,
        json!({"op":"object.set","object":inactive,"active":true}),
    );
    attempt(&w, &c, &deferred, "Awake", "ok");
    attempt(&w, &c, &deferred, "OnEnable", "ok");
    assert!(next(&w, &c).is_null());
}

#[test]
fn lifecycle_and_coroutine_children_share_the_same_parent_restore_stack() {
    let (w, c, parent) = entered_start();
    let clone = add(&w, &c, &object(&w), json!(["Awake", "OnEnable"]));
    let awake = next(&w, &c);
    enter(&w, &c, &awake);
    let routine = child(&w, &c, clone.as_u64().unwrap());
    enter(&w, &c, &routine["immediate"]);
    let grandchild = add(&w, &c, &object(&w), json!(["Awake"]));
    attempt(&w, &c, &grandchild, "Awake", "ok");
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &routine["immediate"], "done");
    let dispose = next(&w, &c);
    assert_eq!(dispose["routine"], routine["routine"]);
    assert_eq!(dispose["kind"], "Dispose");
    enter(&w, &c, &dispose);
    complete(&w, &c, &dispose, "ok");
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &awake, "ok");
    attempt(&w, &c, &clone, "OnEnable", "ok");
    assert!(next(&w, &c).is_null());
    complete(&w, &c, &parent, "ok");
    assert!(next(&w, &c).is_null());
}
