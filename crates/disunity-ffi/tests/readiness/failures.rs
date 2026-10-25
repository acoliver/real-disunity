use super::*;

#[test]
fn cleanup_cannot_close_with_undisposed_native_routines() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    let r = start(&w, &c, s);
    suspend(&w, &c, r, Value::Null);
    finish(&w, &host, &c);
    let c = claim(&w, &host, "Cleanup", 0, 0);
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":host,"context":c}))["ok"],
        false
    );
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    complete(&w, &c, &t, "failed");
    assert!(next(&w, &c).is_null());
    finish(&w, &host, &c);
}
#[test]
fn rational_deadline_overflow_is_atomic_and_suspend_remains_available() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (host, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    finish(&w, &host, &c);
    ok(
        &w,
        json!({"op":"session.configure","host":host,"time_scale":{"num":0,"den":1}}),
    );
    dynamic(&w, &host, 1, |c| {
        let r = start(&w, c, s);
        let t = next(&w, c);
        complete(&w, c, &t, "more");
        let t = next(&w, c);
        complete(&w, c, &t, "ok");
        let before = w.snapshot();
        assert_eq!(
            w.call(json!({"op":"readiness.suspend","context":c,"routine":r,
            "wait":{"kind":"unscaled","seconds":{"num":1,"den":999999999989u64}}}))["ok"],
            false
        );
        assert_eq!(w.snapshot(), before);
        ok(
            &w,
            json!({"op":"readiness.suspend","context":c,"routine":r,"wait":null}),
        );
    });
}
#[test]
fn cancelled_entered_iterator_finishes_attempt_then_disposes_once() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (_, c) = open(&w);
    let (_, s) = script(&w, &c, json!([]));
    let r = start(&w, &c, s);
    let t = next(&w, &c);
    ok(
        &w,
        json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]}),
    );
    ok(&w, json!({"op":"readiness.cancel","context":c,"routine":r}));
    assert_eq!(
        w.call(json!({"op":"readiness.next","context":c}))["ok"],
        false
    );
    ok(
        &w,
        json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"more"}),
    );
    assert_eq!(
        w.call(json!({"op":"readiness.suspend","context":c,"routine":r,"wait":null}))["ok"],
        false
    );
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    complete(&w, &c, &t, "failed");
    ok(&w, json!({"op":"readiness.cancel","context":c,"routine":r}));
    assert!(next(&w, &c).is_null());
}
#[test]
fn routine_cancellation_latches_an_owner_inactivation_before_reactivation() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (_, c) = open(&w);
    let (o, s) = script(&w, &c, json!([]));
    let r = start(&w, &c, s);
    suspend(&w, &c, r, Value::Null);
    ok(&w, json!({"op":"object.set","object":o,"active":false}));
    ok(&w, json!({"op":"object.set","object":o,"active":true}));
    let t = next(&w, &c);
    assert_eq!(t["kind"], "Dispose");
    complete(&w, &c, &t, "ok");
}
