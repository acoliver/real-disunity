use super::*;
use worldrunner_disunity_ffi::{DisunityCommand, abi::*};

fn advances(w: &World, request: Value) -> Value {
    let before = revision(w);
    let result = ok(w, request);
    assert_eq!(revision(w), before + 1);
    result
}
#[test]
fn typed_mutation_families_advance_exactly_once_in_a_claimed_root() {
    let w = World::new();
    let (h, c) = setup(&w);
    let o = advances(&w, json!({"op":"object.create"}))["handle"].clone();
    let p = advances(&w, json!({"op":"object.create"}))["handle"].clone();
    let body = advances(
        &w,
        json!({"op":"component.add","object":o,"kind":"Rigidbody2D"}),
    )["handle"]
        .clone();
    for request in [
        json!({"op":"object.set","object":o,"name":"identity","tag":"tag","layer":2}),
        json!({"op":"transform.set","object":o,"position":[1,2,3],"rotation":[0,0,0,1],"scale":[2,3,4]}),
        json!({"op":"transform.parent","object":o,"parent":p}),
        json!({"op":"component.set","component":body,"fields":{"mass":2}}),
        json!({"op":"body.move","component":body,"position":[1,2]}),
        json!({"op":"body.force","component":body,"force":[1,2],"mode":"force"}),
        json!({"op":"body.sleep","component":body}),
        json!({"op":"body.wake","component":body}),
        json!({"op":"physics.layers","layer_a":1,"layer_b":2,"ignore":true}),
        json!({"op":"physics.configure","gravity":[0,-1]}),
        json!({"op":"events.read","clear":true}),
        json!({"op":"input.set","keys":["W"]}),
        json!({"op":"input.configure_axes","axes":{}}),
        json!({"op":"input.focus","focused":false}),
        json!({"op":"random.seed","seed":1}),
        json!({"op":"random.range","min":0,"max":1}),
        json!({"op":"ui.configure","viewport":[65,47]}),
        json!({"version":1,"op":"transaction","operations":[{"op":"random.range","min":0,"max":1},{"op":"object.set","object":o,"active":false}]}),
        json!({"op":"component.remove","component":body}),
        json!({"op":"object.clone","object":o}),
        json!({"op":"object.destroy","object":p}),
    ] {
        advances(&w, request);
    }
    let s = advances(&w, json!({"op":"component.add","object":ok(&w,json!({"op":"object.create"}))["handle"],"kind":"Script"}))["handle"].clone();
    advances(
        &w,
        json!({"op":"readiness.register","context":c,"component":s,"callbacks":[]}),
    );
    advances(&w, json!({"op":"readiness.next","context":c}));
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    advances(
        &w,
        json!({"op":"session.configure","host":h,"time_scale":{"num":0,"den":1}}),
    );
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], false);
    assert_eq!(valid(&w, &h, &a["stamp"], false)["ok"], true);
}
#[test]
fn failed_rng_transaction_preserves_draw_and_current_snapshot() {
    let w = World::new();
    let (h, c) = setup(&w);
    ok(&w, json!({"op":"random.seed","seed":42}));
    let draw = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    let expected = ok(&w, draw.clone());
    ok(&w, json!({"op":"random.seed","seed":42}));
    let before = revision(&w);
    assert_eq!(w.call(json!({"version":1,"op":"transaction","operations":[draw.clone(),{"op":"object.set","object":0}]}))["ok"], false);
    assert_eq!(revision(&w), before);
    assert_eq!(ok(&w, draw), expected);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    for request in [
        json!({"op":"session.configure","host":h,"time_scale":{"num":-1,"den":1}}),
        json!({"version":1,"op":"transaction","operations":[{"op":"random.seed","seed":123},{"op":"object.set","object":0}]}),
        json!({"op":"input.events","events":[{"kind":"key"}]}),
    ] {
        assert_eq!(w.call(request)["ok"], false);
    }
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
}
#[test]
fn direct_abi_creation_queue_and_destruction_share_revision_barriers() {
    let w = World::new();
    let mut o = 0;
    let before = revision(&w);
    assert_eq!(unsafe { disunity_gameobject_create(w.0, &mut o) }.code, 0);
    assert_eq!(revision(&w), before + 1);
    let command = DisunityCommand {
        object: o,
        kind: 0,
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(w.0, &command, 1) }.code,
        0
    );
    assert_eq!(revision(&w), before + 2);
    assert_eq!(unsafe { disunity_world_step(w.0, 0.01, 1) }.code, 0);
    assert_eq!(revision(&w), before + 3);
    assert_eq!(
        unsafe { disunity_transform_set_rotation_y(w.0, o, 0.5) }.code,
        0
    );
    assert_eq!(revision(&w), before + 4);
    assert_eq!(unsafe { disunity_gameobject_destroy(w.0, o) }.code, 0);
    assert_eq!(revision(&w), before + 5);
    assert_ne!(unsafe { disunity_gameobject_destroy(w.0, o) }.code, 0);
    assert_eq!(revision(&w), before + 5);
    assert_eq!(w.snapshot()["objects"], json!([]));
}
#[test]
fn strict_direct_step_and_unclaimed_mutation_reject_without_revision_change() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    let mut o = 0;
    let before = revision(&w);
    assert_ne!(unsafe { disunity_world_step(w.0, 0.1, 1) }.code, 0);
    assert_ne!(unsafe { disunity_gameobject_create(w.0, &mut o) }.code, 0);
    assert_ne!(
        unsafe { disunity_command_buffer_submit(w.0, std::ptr::null(), 0) }.code,
        0
    );
    assert_eq!(revision(&w), before);
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], true);
}
#[test]
fn native_only_descriptor_preserves_transport_limits() {
    let w = World::new();
    let d = ok(&w, json!({"op":"describe"}));
    assert!(
        d["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("snapshot_primitives_v1"))
    );
    assert_eq!(d["snapshot_primitives"]["managed_readiness"], false);
    assert_eq!(d["snapshot_primitives"]["pixel_attestation"], false);
    assert_eq!(d["limits"]["max_request_bytes"], 1 << 20);
    assert_eq!(d["limits"]["max_response_bytes"], 16 << 20);
    assert_eq!(d["limits"]["max_live_response_bytes"], 64 << 20);
    assert_eq!(d["limits"]["max_responses"], 1024);
}

#[test]
fn idle_timestamped_input_admission_invalidates_current_and_failure_preserves_it() {
    let w = World::new();
    let (h, c) = setup(&w);
    idle(&w, &h, &c);
    let a = capture(&w, &h);
    advances(
        &w,
        json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":0,"den":1},"source":"os","kind":"key","key":"W","pressed":true}]}),
    );
    assert_eq!(valid(&w, &h, &a["stamp"], true)["ok"], false);
    assert_eq!(valid(&w, &h, &a["stamp"], false)["ok"], true);
    let b = capture(&w, &h);
    assert_eq!(
        w.call(json!({"op":"input.events","events":[]}))["ok"],
        false
    );
    assert_eq!(valid(&w, &h, &b["stamp"], true)["ok"], true);
}
