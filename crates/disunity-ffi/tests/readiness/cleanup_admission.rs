use super::*;
use worldrunner_disunity_ffi::response::{request_bytes, response_bytes, response_free};

fn filler(w: &World) -> Vec<u8> {
    let large = ok(w, json!({"op":"object.create","name":"x".repeat(600_000)}))["handle"].clone();
    let tail = ok(w, json!({"op":"object.create","name":""}))["handle"].clone();
    let mut operations = vec![json!({"op":"object.get","object":large}); 13];
    operations.push(json!({"op":"object.get","object":tail}));
    let request = json!({"version":1,"op":"transaction","operations":operations});
    let base = serde_json::to_vec(&w.call(request.clone())).unwrap().len();
    ok(
        w,
        json!({"op":"object.set","object":tail,"name":"p".repeat((8 << 20)-base)}),
    );
    serde_json::to_vec(&request).unwrap()
}

fn read(w: &World, request: Value) -> Value {
    let response = worldrunner_disunity_ffi::request_value(w.0, request).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["data"].clone()
}

fn reject_at_byte_limit(w: &World, fill: &[u8], request: Value) {
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
fn cleanup_full_generation_byte_quota_rolls_back_all_ticket_transitions() {
    let _table = crate::response_table::exclusive();
    let w = World::new();
    let (host, c, _) = setup(&w);
    let fill = filler(&w);
    reject_at_byte_limit(&w, &fill, json!({"op":"readiness.next","context":c}));
    for _ in 0..4 {
        let t = next(&w, &c);
        let enter = json!({"op":"readiness.enter","context":c,"ticket":t["ticket"]});
        reject_at_byte_limit(&w, &fill, enter.clone());
        assert_eq!(ok(&w, enter)["execute"], true);
        let done =
            json!({"op":"readiness.complete","context":c,"ticket":t["ticket"],"outcome":"ok"});
        reject_at_byte_limit(&w, &fill, done.clone());
        ok(&w, done);
    }
    assert!(next(&w, &c).is_null());
    reject_at_byte_limit(
        &w,
        &fill,
        json!({"op":"phase.finish","host":host,"context":c}),
    );
    finish(&w, &host, &c);
}
