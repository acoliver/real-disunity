use super::*;
use worldrunner_disunity_ffi::response::{request_bytes, response_bytes, response_free};
fn read(w: &World, r: Value) -> Value {
    let v = worldrunner_disunity_ffi::request_value(w.0, r).unwrap();
    assert_eq!(v["ok"], true);
    v["data"].clone()
}
fn filler(w: &World) -> Vec<u8> {
    let large = ok(w, json!({"op":"object.create","name":"x".repeat(600_000)}))["handle"].clone();
    let tail = ok(w, json!({"op":"object.create","name":""}))["handle"].clone();
    let mut operations = vec![json!({"op":"object.get","object":large}); 13];
    operations.push(json!({"op":"object.get","object":tail}));
    let r = json!({"version":1,"op":"transaction","operations":operations});
    let base = serde_json::to_vec(&w.call(r.clone())).unwrap().len();
    ok(
        w,
        json!({"op":"object.set","object":tail,"name":"p".repeat((8 << 20)-base)}),
    );
    serde_json::to_vec(&r).unwrap()
}
fn quota_reject(w: &World, fill: &[u8], request: Value) {
    let first = request_bytes(w.0, fill).unwrap();
    let second = request_bytes(w.0, fill).unwrap();
    assert_eq!(response_bytes(first).unwrap().len(), 8 << 20);
    assert_eq!(response_bytes(second).unwrap().len(), 8 << 20);
    let before = read(w, json!({"op":"session.status"}));
    assert_eq!(
        request_bytes(w.0, &serde_json::to_vec(&request).unwrap()),
        Err(2)
    );
    assert_eq!(read(w, json!({"op":"session.status"})), before);
    response_free(first).unwrap();
    response_free(second).unwrap();
}
#[test]
fn full_target_sixteen_mib_admission_rolls_back_reserve_accept_enter_complete_finish() {
    let w = World::new();
    ok(
        &w,
        json!({"version":1,"op":"transport.select","transport_profile":"transport_byte_limits_v1"}),
    );
    let (_, c, _) = setup(&w, 1);
    let fill = filler(&w);
    quota_reject(&w, &fill, json!({"op":"callbacks.next","context":c}));
    let e = batch(&w, &c);
    let b = e["context"]["callback_batch"].clone();
    let before = inspect(&w, &c, &e);
    quota_reject(
        &w,
        &fill,
        json!({"op":"callbacks.accept","context":c,"envelope":e}),
    );
    assert_eq!(inspect(&w, &c, &e), before);
    accept(&w, &c, &e);
    for i in 0..2 {
        let before = inspect(&w, &c, &e);
        quota_reject(
            &w,
            &fill,
            json!({"op":"callbacks.enter","context":c,"batch":b,"index":i}),
        );
        assert_eq!(inspect(&w, &c, &e), before);
        let t = enter(&w, &c, &e, i);
        quota_reject(
            &w,
            &fill,
            json!({"op":"readiness.complete","context":c,"ticket":t["decision"]["ticket"],"outcome":"ok"}),
        );
        assert_eq!(inspect(&w, &c, &e)["cursor"], i);
        complete(&w, &c, &t, "ok");
    }
    quota_reject(
        &w,
        &fill,
        json!({"op":"callbacks.finish","context":c,"batch":b}),
    );
    assert_eq!(inspect(&w, &c, &e)["cursor"], 2);
    finish_batch(&w, &c, &e);
    assert!(batch(&w, &c).is_null());
}
#[test]
fn terminal_takeover_revokes_batch_after_unknown_entered_attempt_without_replay() {
    let w = World::new();
    let (host, c, _) = setup(&w, 2);
    let e = batch(&w, &c);
    accept(&w, &c, &e);
    enter(&w, &c, &e, 0);
    let r = ok(
        &w,
        json!({"op":"session.begin_cleanup","host":host,"reason":"infrastructure_failure"}),
    );
    assert_eq!(r["stamp"]["terminal_recovery"]["execution_unknown"], 1);
    let fresh = r["context"].clone();
    ok(
        &w,
        json!({"op":"phase.claim","phase":"Cleanup","context":fresh}),
    );
    assert_eq!(
        w.call(
            json!({"op":"callbacks.inspect","context":c,"batch":e["context"]["callback_batch"]})
        )["ok"],
        false
    );
    let remaining = batch(&w, &fresh);
    assert_eq!(remaining["callbacks"].as_array().unwrap().len(), 3);
    accept(&w, &fresh, &remaining);
    for i in 0..3 {
        let t = enter(&w, &fresh, &remaining, i);
        complete(&w, &fresh, &t, "ok");
    }
    finish_batch(&w, &fresh, &remaining);
    assert_eq!(
        ok(&w, json!({"op":"phase.finish","host":host,"context":fresh}))["stage"],
        "Closed"
    );
}
