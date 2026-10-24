use super::*;
use worldrunner_disunity_ffi::abi::*;

#[test]
fn receipt_count_is_bounded_and_release_recovers_capacity_without_id_reuse() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let mut held = Vec::new();
    for _ in 0..128 {
        held.push(capture(&w, &h)["stamp"].clone());
    }
    let before = revision(&w);
    assert_eq!(
        ok(&w, json!({"op":"session.status"}))["snapshot_receipts"],
        128
    );
    assert_eq!(
        w.call(json!({"op":"snapshot.capture","host":h}))["ok"],
        false
    );
    assert_eq!(revision(&w), before);
    ok(
        &w,
        json!({"op":"snapshot.release","host":h,"stamp":held[0]}),
    );
    assert_eq!(
        w.call(json!({"op":"snapshot.release","host":h,"stamp":held[0]}))["ok"],
        false
    );
    let next = capture(&w, &h);
    assert_eq!(next["stamp"]["snapshot_id"], 129);
    assert_eq!(valid(&w, &h, &held[0], false)["ok"], false);
    assert_eq!(valid(&w, &h, &held[127], true)["ok"], true);
}
#[test]
fn represented_bytes_are_bounded_independently_of_response_lifetime() {
    let w = World::new();
    let (h, c) = setup(&w);
    ok(&w, json!({"op":"object.create","name":"n".repeat(700_000)}));
    idle(&w, &h, &c);
    let mut held = Vec::new();
    loop {
        let result = w.call(json!({"op":"snapshot.capture","host":h}));
        if result["ok"] == false {
            assert!(
                result["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("byte budget")
            );
            break;
        }
        held.push(result["data"]["stamp"].clone());
        assert!(held.len() < 128);
    }
    let status = ok(&w, json!({"op":"session.status"}));
    assert!(status["snapshot_bytes"].as_u64().unwrap() <= 32 << 20);
    ok(
        &w,
        json!({"op":"snapshot.release","host":h,"stamp":held[0]}),
    );
    capture(&w, &h);
}
#[test]
fn immutable_response_survives_world_mutation_release_destroy_and_cross_thread_copy() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let bytes = serde_json::to_vec(&json!({"op":"snapshot.capture","host":h})).unwrap();
    let (code, response) = support::submit(w.0, &bytes);
    assert_eq!(code, 0);
    let original = support::copy(response);
    let a: Value = serde_json::from_slice(&original).unwrap();
    ok(
        &w,
        json!({"op":"session.configure","host":h,"time_scale":{"num":0,"den":1}}),
    );
    assert_eq!(support::copy(response), original);
    ok(
        &w,
        json!({"op":"snapshot.release","host":h,"stamp":a["data"]["stamp"]}),
    );
    let dead = w.0;
    drop(w);
    let copied = std::thread::spawn(move || {
        let copied = support::copy(response);
        assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
        copied
    })
    .join()
    .unwrap();
    assert_eq!(original, copied);
    let (code, response) = support::submit(dead, &bytes);
    assert_eq!((code, response), (1, 0));
}
#[test]
fn oversized_capture_rolls_back_receipt_issuance_and_id() {
    let w = World::new();
    let (h, c) = setup(&w);
    let mut objects = Vec::new();
    for _ in 0..18 {
        objects.push(
            ok(&w, json!({"op":"object.create","name":"n".repeat(950_000)}))["handle"].clone(),
        );
    }
    idle(&w, &h, &c);
    let before = ok(&w, json!({"op":"session.status"}));
    let result = w.call(json!({"op":"snapshot.capture","host":h}));
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("response budget")
    );
    assert_eq!(ok(&w, json!({"op":"session.status"})), before);
    assert_eq!(before["snapshot_receipts"], 0);
    ok(
        &w,
        json!({"op":"frame.open","host":h,"dt":{"num":0,"den":1}}),
    );
    let c = super::authority::claim(&w, &h, "Update", 1, 0);
    for o in objects {
        ok(&w, json!({"op":"object.set","object":o,"name":"small"}));
    }
    idle(&w, &h, &c);
    for phase in ["UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
        let c = super::authority::claim(&w, &h, phase, 1, 0);
        idle(&w, &h, &c);
    }
    ok(&w, json!({"op":"frame.end","host":h,"frame":1}));
    assert_eq!(capture(&w, &h)["stamp"]["snapshot_id"], 1);
}
