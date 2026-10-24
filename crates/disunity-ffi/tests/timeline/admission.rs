use crate::{World, begin, enqueue, input, json, key, ok, ratio, select};
use worldrunner_disunity_ffi::abi::*;

#[test]
fn late_service_selection_rejects_past_input_without_selecting_mode() {
    let w = World::new();
    begin(&w, 1, 10);
    let before = ok(&w, json!({"op":"input.status"}));
    assert_eq!(
        w.call(json!({"op":"input.events","events":[key(1,0,1,"W",true)]}))["ok"],
        false
    );
    assert_eq!(ok(&w, json!({"op":"input.status"})), before);
    enqueue(&w, vec![key(1, 1, 5, "W", true)]);
    assert_eq!(input(&w, "dynamic")["timestamp"], ratio(1, 10));
}

#[test]
fn legacy_world_step_rejection_preserves_pending_commands_and_clock() {
    let w = World::new();
    let object = ok(&w, json!({"op":"object.create"}))["handle"]
        .as_u64()
        .unwrap();
    select(&w);
    let command = DisunityCommand {
        object,
        kind: 1,
        x: 9.0,
        y: 0.0,
        z: 0.0,
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(w.0, &command, 1) }.code,
        0
    );
    let before = w.snapshot();
    assert_ne!(unsafe { disunity_world_step(w.0, 0.1, 1) }.code, 0);
    assert_eq!(w.snapshot(), before);
}

#[test]
fn button_and_axis_events_pause_controls_and_transaction_commit() {
    let w = World::new();
    select(&w);
    ok(
        &w,
        json!({"op":"time.set_rational","time_scale":ratio(0,1)}),
    );
    let events = vec![
        json!({"kind":"button","button":"Mouse0","pressed":true,
        "source":"os","timestamp":ratio(1,10),"event_id":1}),
        json!({"kind":"axis","axis":"Horizontal","value":0.5,
        "source":"replay","timestamp":ratio(1,10),"event_id":2}),
    ];
    let result = ok(
        &w,
        json!({"version":1,"op":"transaction","operations":[
        {"op":"input.events","events":events},{"op":"frame.begin_rational","dt":ratio(1,10)},
        {"op":"input.get"},{"op":"time.get"}]}),
    );
    assert_eq!(result[2]["down"], json!(["Mouse0"]));
    assert_eq!(result[2]["axes"]["Horizontal"], 0.5);
    assert_eq!(result[3]["time"], 0.0);
    assert_eq!(result[3]["unscaled_time"], 0.1);
    enqueue(
        &w,
        vec![json!({"kind":"focus","focused":false,
        "source":"os","timestamp":ratio(1,5),"event_id":3})],
    );
    begin(&w, 1, 10);
    assert_eq!(input(&w, "dynamic")["up"], json!(["Mouse0"]));
    assert!(input(&w, "dynamic")["axes"].as_object().unwrap().is_empty());
}

#[test]
fn u64_sequence_exhaustion_and_oversized_batch_are_transactional() {
    let w = World::new();
    select(&w);
    enqueue(&w, vec![key(u64::MAX, 0, 1, "W", true)]);
    let status = ok(&w, json!({"op":"input.status"}));
    for id in [0, 1, u64::MAX] {
        assert_eq!(
            w.call(json!({"op":"input.events","events":[key(id,1,10,"W",false)]}))["ok"],
            false
        );
        assert_eq!(ok(&w, json!({"op":"input.status"})), status);
    }
    let other = World::new();
    select(&other);
    let events: Vec<_> = (1..=4097).map(|id| key(id, 0, 1, "W", true)).collect();
    assert_eq!(
        other.call(json!({"op":"input.events","events":events}))["ok"],
        false
    );
    assert_eq!(ok(&other, json!({"op":"input.status"}))["mode"], "legacy");
}

#[test]
fn timestamp_order_checks_are_independent_of_id_and_projection_checks() {
    let w = World::new();
    select(&w);
    enqueue(&w, vec![key(1, 1, 10, "W", true)]);
    let before = ok(&w, json!({"op":"input.status"}));
    for events in [
        vec![key(2, 1, 20, "W", false)],
        vec![key(2, 3, 10, "D", true), key(3, 2, 10, "D", false)],
    ] {
        let response = w.call(json!({"op":"input.events","events":events}));
        assert_eq!(response["ok"], false);
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("timestamps must not decrease")
        );
        assert_eq!(ok(&w, json!({"op":"input.status"})), before);
    }
}

#[test]
fn held_key_budget_rejects_whole_batch_before_mode_selection() {
    let w = World::new();
    select(&w);
    let events: Vec<_> = (1..=257)
        .map(|id| key(id, 0, 1, &format!("Key{id}"), true))
        .collect();
    let before = ok(&w, json!({"op":"input.status"}));
    let response = w.call(json!({"op":"input.events","events":events}));
    assert_eq!(response["ok"], false);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("held key/axis budget")
    );
    assert_eq!(ok(&w, json!({"op":"input.status"})), before);
}
