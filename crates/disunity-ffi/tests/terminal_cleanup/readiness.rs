use super::*;

fn next(w: &World, c: &Value) -> Value {
    ok(w, json!({"op":"readiness.next","context":c}))
}
fn enter(w: &World, c: &Value, t: &Value) {
    ok(
        w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
}
fn complete(w: &World, c: &Value, t: &Value, outcome: &str) {
    ok(
        w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":outcome}),
    );
}
fn setup(w: &World, c: &Value, callbacks: Value) -> (Value, Value) {
    let o = ok(w, json!({"op":"object.create"}))["handle"].clone();
    let s = ok(w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    ok(
        w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":callbacks}),
    );
    (o, s)
}

#[test]
fn recovery_abandons_iterator_pipeline_and_never_retries_entered_dispose() {
    let _table = crate::response_table::exclusive();
    for kind in ["MoveNext", "Current", "Dispose", "Suspend", "Terminal"] {
        exercise_pipeline(kind, false);
        exercise_pipeline(kind, true);
    }
}

fn exercise_pipeline(kind: &str, entered: bool) {
    let w = World::new();
    let h = open(&w);
    let old = issue(&w, &h, "Initialize");
    claim(&w, &old);
    let (o, s) = setup(&w, &old, json!([]));
    let r = ok(
        &w,
        json!({"op":"readiness.start","context":old,"component":s}),
    )["routine"]
        .clone();
    let mut t = next(&w, &old);
    if matches!(kind, "Current" | "Suspend") {
        enter(&w, &old, &t);
        complete(&w, &old, &t, "more");
        t = next(&w, &old);
    }
    if matches!(kind, "Dispose" | "Terminal") {
        enter(&w, &old, &t);
        complete(&w, &old, &t, "done");
        t = next(&w, &old);
    }
    if entered || matches!(kind, "Suspend" | "Terminal") {
        enter(&w, &old, &t);
    }
    if matches!(kind, "Suspend" | "Terminal") {
        complete(&w, &old, &t, "ok");
    }
    ok(&w, json!({"op":"object.destroy","object":o}));
    let c = begin(&w, &h);
    claim(&w, &c);
    reject(
        &w,
        json!({"op":"readiness.complete","context":old,"ticket":t["ticket"],"outcome":"ok"}),
    );
    let d = next(&w, &c);
    if kind == "Terminal" || (kind == "Dispose" && entered) {
        assert!(d.is_null());
    } else {
        assert_eq!(d["kind"], "Dispose");
        assert_eq!(d["routine"], r);
        reject(&w, json!({"op":"phase.finish","host":h,"context":c}));
        enter(&w, &c, &d);
        complete(&w, &c, &d, "failed");
        assert!(next(&w, &c).is_null());
    }
    finish(&w, &h, &c);
}

#[test]
fn recovery_preserves_entered_lifecycle_once_flags_and_tombstones() {
    let _table = crate::response_table::exclusive();
    for kind in ["Awake", "OnEnable", "OnDisable", "OnDestroy"] {
        let w = World::new();
        let h = open(&w);
        let old = issue(&w, &h, "Initialize");
        claim(&w, &old);
        let (o, _) = setup(
            &w,
            &old,
            json!(["Awake", "OnEnable", "OnDisable", "OnDestroy"]),
        );
        let mut t = next(&w, &old);
        for completed in ["Awake", "OnEnable"] {
            if kind == completed {
                break;
            }
            enter(&w, &old, &t);
            complete(&w, &old, &t, "ok");
            if completed == "OnEnable" {
                ok(&w, json!({"op":"object.destroy","object":o}));
            }
            t = next(&w, &old);
        }
        if kind == "OnDestroy" {
            enter(&w, &old, &t);
            complete(&w, &old, &t, "failed");
            t = next(&w, &old);
        }
        assert_eq!(t["kind"], kind);
        enter(&w, &old, &t);
        let c = begin(&w, &h);
        claim(&w, &c);
        let mut seen = Vec::new();
        loop {
            let t = next(&w, &c);
            if t.is_null() {
                break;
            }
            seen.push(t["kind"].as_str().unwrap().to_owned());
            enter(&w, &c, &t);
            complete(&w, &c, &t, "failed");
        }
        assert!(!seen.iter().any(|s| s == kind));
        assert!(
            seen.iter()
                .all(|s| matches!(s.as_str(), "OnDisable" | "OnDestroy"))
        );
        finish(&w, &h, &c);
    }
}
