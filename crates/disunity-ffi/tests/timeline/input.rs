use crate::{World, begin, enqueue, input, json, key, ok, ratio, select};

fn assert_tap(snapshot: &serde_json::Value) {
    assert_eq!(snapshot["down"], json!(["W"]));
    assert_eq!(snapshot["up"], json!(["W"]));
    assert_eq!(snapshot["keys"], json!([]));
}

#[test]
fn short_tap_survives_both_independent_consumers_and_polls_are_stable() {
    let w = World::new();
    select(&w);
    enqueue(
        &w,
        vec![key(1, 1, 1000, "W", true), key(2, 2, 1000, "W", false)],
    );
    assert_eq!(input(&w, "dynamic")["down"], json!([]));
    assert_eq!(begin(&w, 1, 100)["fixed_steps"], 0);
    let dynamic = input(&w, "dynamic");
    assert_tap(&dynamic);
    assert_eq!(dynamic, input(&w, "dynamic"));
    assert_eq!(input(&w, "fixed")["down"], json!([]));
    assert_eq!(begin(&w, 3, 100)["fixed_steps"], 2);
    assert_eq!(input(&w, "dynamic")["down"], json!([]));
    ok(&w, json!({"op":"physics.step"}));
    let fixed = input(&w, "fixed");
    assert_eq!(fixed["down"], json!(["W"]));
    assert_eq!(fixed["up"], json!(["W"]));
    assert_eq!(fixed, input(&w, "fixed"));
    ok(&w, json!({"op":"physics.step"}));
    assert_eq!(input(&w, "fixed")["down"], json!([]));
    assert_eq!(input(&w, "fixed")["projection_id"], 2);
    assert_eq!(ok(&w, json!({"op":"input.status"}))["queued_events"], 0);
}

#[test]
fn source_union_repeat_axes_both_held_and_global_focus_loss() {
    let w = World::new();
    select(&w);
    ok(
        &w,
        json!({"op":"input.configure_axes","axes":{"Horizontal":{
        "negative_keys":["A"],"positive_keys":["D"]}}}),
    );
    let mut replay = key(2, 0, 1, "D", true);
    replay["source"] = json!("replay");
    enqueue(
        &w,
        vec![
            key(1, 0, 1, "A", true),
            replay,
            key(3, 0, 1, "D", true),
            key(4, 0, 1, "D", true),
        ],
    );
    begin(&w, 1, 60);
    let both = input(&w, "dynamic");
    assert_eq!(both["keys"], json!(["A", "D"]));
    assert_eq!(both["down"], json!(["A", "D"]));
    assert_eq!(both["axes"]["Horizontal"], 0.0);
    enqueue(&w, vec![key(5, 1, 50, "D", false)]);
    begin(&w, 1, 60);
    assert_eq!(input(&w, "dynamic")["keys"], json!(["A", "D"]));
    assert_eq!(input(&w, "dynamic")["up"], json!([]));
    enqueue(
        &w,
        vec![
            json!({"event_id":6,"timestamp":ratio(1,25),"source":"os","kind":"focus","focused":false}),
            key(7, 1, 25, "W", true),
        ],
    );
    begin(&w, 1, 60);
    let lost = input(&w, "dynamic");
    assert_eq!(lost["up"], json!(["A", "D"]));
    assert_eq!(lost["keys"], json!([]));
    assert_eq!(lost["focused"], false);
    enqueue(
        &w,
        vec![
            json!({"event_id":8,"timestamp":ratio(3,50),"source":"os","kind":"focus","focused":true}),
            key(9, 3, 50, "D", true),
        ],
    );
    begin(&w, 1, 60);
    assert_eq!(input(&w, "dynamic")["down"], json!(["D"]));
    assert_eq!(input(&w, "dynamic")["axes"]["Horizontal"], 1.0);
}

#[test]
fn future_events_and_configuration_do_not_mutate_published_snapshots() {
    let w = World::new();
    select(&w);
    enqueue(&w, vec![key(1, 1, 2, "W", true)]);
    begin(&w, 1, 4);
    let before = input(&w, "dynamic");
    assert_eq!(before["keys"], json!([]));
    ok(
        &w,
        json!({"op":"input.configure_axes","axes":{"Vertical":{"positive_keys":["W"]}}}),
    );
    assert_eq!(input(&w, "dynamic"), before);
    begin(&w, 1, 4);
    assert_eq!(input(&w, "dynamic")["axes"]["Vertical"], 1.0);
    assert_eq!(input(&w, "fixed")["keys"], json!([]));
}

#[test]
fn invalid_ingress_order_names_bounds_and_legacy_mix_are_atomic() {
    let w = World::new();
    select(&w);
    enqueue(&w, vec![key(1, 1, 100, "W", true)]);
    begin(&w, 1, 50);
    let status = ok(&w, json!({"op":"input.status"}));
    for event in [
        key(1, 1, 10, "A", true),
        key(2, 1, 100, "A", true),
        key(2, 0, 1, "A", true),
        key(2, 1, 10, "", true),
        json!({"event_id":2,"timestamp":ratio(1,10),"source":"os","kind":"axis","axis":"X","value":2}),
        json!({"event_id":2,"timestamp":ratio(1,10),"source":"unknown","kind":"key","key":"A","pressed":true}),
    ] {
        let before = w.snapshot();
        let response = w.call(json!({"op":"input.events","events":[key(2,1,10,"D",true),event]}));
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(w.snapshot(), before);
        assert_eq!(ok(&w, json!({"op":"input.status"})), status);
    }
    assert_eq!(w.call(json!({"op":"input.set","keys":[]}))["ok"], false);
    assert_eq!(
        w.call(json!({"op":"input.focus","focused":false}))["ok"],
        false
    );
}

#[test]
fn event_queue_budget_and_request_identity_are_independent() {
    let w = World::new();
    select(&w);
    let events: Vec<_> = (1..=4096).map(|id| key(id, 0, 1, "W", true)).collect();
    let request = json!({"version":1,"requestId":"ingress","op":"input.events","events":events});
    let result = w.call(request.clone());
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(w.call(request), result);
    assert_eq!(
        w.call(json!({"op":"input.events","events":[key(4097,0,1,"W",false)]}))["ok"],
        false
    );
    begin(&w, 1, 50);
    assert_eq!(ok(&w, json!({"op":"input.status"}))["queued_events"], 4096);
    ok(&w, json!({"op":"physics.step"}));
    enqueue(&w, vec![key(4097, 1, 25, "W", false)]);
}
