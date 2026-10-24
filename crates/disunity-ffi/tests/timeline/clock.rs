use crate::{World, begin, json, ok, ratio, select};

#[test]
fn exact_six_frames_admit_five_and_only_solver_steps_advance_fixed_identity() {
    let w = World::new();
    let object = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    ok(
        &w,
        json!({"op":"component.add","object":object,"kind":"Rigidbody2D",
        "fields":{"velocity":[1,0],"gravity_scale":0}}),
    );
    let mut admissions = 0;
    for _ in 0..6 {
        let frame = begin(&w, 1, 60);
        admissions += frame["fixed_steps"].as_u64().unwrap();
        for _ in 0..frame["fixed_steps"].as_u64().unwrap() {
            ok(&w, json!({"op":"physics.step"}));
        }
    }
    let t = ok(&w, json!({"op":"time.get"}));
    assert_eq!(admissions, 5);
    assert_eq!(t["rational"]["time"], ratio(1, 10));
    assert_eq!(t["rational"]["unscaled_time"], ratio(1, 10));
    assert_eq!(t["rational"]["accumulator"], ratio(0, 1));
    assert_eq!(t["fixed_frame"], 5);
    let p = ok(&w, json!({"op":"transform.get","object":object}));
    assert!((p["position"][0].as_f64().unwrap() - 0.1).abs() < 1e-6);
}

#[test]
fn long_run_exact_conservation_and_reduced_equivalent_inputs() {
    let w = World::new();
    let mut steps = 0;
    for _ in 0..6000 {
        steps += begin(&w, 2, 120)["fixed_steps"].as_u64().unwrap();
    }
    let t = ok(&w, json!({"op":"time.get"}));
    assert_eq!(steps, 5000);
    assert_eq!(t["rational"]["time"], ratio(100, 1));
    assert_eq!(t["rational"]["admitted_fixed_time"], ratio(100, 1));
    assert_eq!(t["rational"]["accumulator"], ratio(0, 1));
    assert_eq!(t["fixed_frame"], 0);
}

#[test]
fn paused_scaled_time_stops_while_unscaled_time_advances() {
    let w = World::new();
    begin(&w, 1, 100);
    ok(
        &w,
        json!({"op":"time.set_rational","time_scale":ratio(0,1)}),
    );
    let paused = begin(&w, 1, 1);
    assert_eq!(paused["fixed_steps"], 0);
    assert_eq!(paused["rational"]["time"], ratio(1, 100));
    assert_eq!(paused["rational"]["unscaled_time"], ratio(101, 100));
    assert_eq!(paused["rational"]["accumulator"], ratio(1, 100));
    assert_eq!(paused["unscaled_delta_time"], 1.0);
    ok(
        &w,
        json!({"op":"time.set_rational","time_scale":ratio(2,1)}),
    );
    assert_eq!(begin(&w, 1, 200)["fixed_steps"], 1);
    let fixed = ok(&w, json!({"op":"time.phase","phase":"fixed"}));
    assert_eq!(fixed["rational"]["delta_time"], ratio(1, 100));
    assert!((fixed["delta_time"].as_f64().unwrap() - 0.02).abs() < 1e-6);
}

#[test]
fn invalid_rationals_and_mode_mixing_preserve_time_and_admission() {
    let w = World::new();
    select(&w);
    for dt in [
        ratio(1, 0),
        ratio(1, 1_000_000_000_001),
        ratio(2, 1),
        ratio(9_007_199_254_740_992, 1),
        json!({"num":1,"den":60,"extra":0}),
    ] {
        let before = w.snapshot();
        assert_eq!(
            w.call(json!({"op":"frame.begin_rational","dt":dt}))["ok"],
            false
        );
        assert_eq!(w.snapshot(), before);
    }
    for request in [
        json!({"op":"time.set_rational","fixed_delta_time":ratio(0,1)}),
        json!({"op":"time.set_rational","time_scale":ratio(101,1)}),
        json!({"op":"frame.begin","dt":0.1}),
        json!({"op":"time.set","time_scale":1}),
    ] {
        let before = w.snapshot();
        assert_eq!(w.call(request)["ok"], false);
        assert_eq!(w.snapshot(), before);
    }
    let old = World::new();
    ok(&old, json!({"op":"frame.begin","dt":0.1}));
    assert_eq!(
        old.call(json!({"op":"frame.begin_rational","dt":ratio(1,60)}))["ok"],
        false
    );
}

#[test]
fn catchup_and_arithmetic_bounds_rollback_world_rng_input_and_retained_response() {
    let w = World::new();
    select(&w);
    ok(
        &w,
        json!({"op":"time.set_rational","fixed_delta_time":ratio(1,1_000_000_000)}),
    );
    let before = w.snapshot();
    let request = json!({"version":1,"requestId":"rollback","op":"transaction","operations":[
        {"op":"object.create","name":"must rollback"},{"op":"random.range","min":0,"max":1},
        {"op":"input.events","events":[crate::key(1,0,1,"W",true)]},
        {"op":"frame.begin_rational","dt":ratio(1,1)}]});
    let failure = w.call(request.clone());
    assert_eq!(failure["ok"], false);
    assert_eq!(w.snapshot(), before);
    assert_eq!(ok(&w, json!({"op":"input.status"}))["last_event_id"], 0);
    ok(
        &w,
        json!({"op":"time.set_rational","fixed_delta_time":ratio(1,50)}),
    );
    assert_eq!(w.call(request), failure);
    let control = World::new();
    assert_eq!(
        ok(&w, json!({"op":"random.range","min":0,"max":1})),
        ok(&control, json!({"op":"random.range","min":0,"max":1}))
    );
    begin(&w, 1, 999_999_937);
    let before = w.snapshot();
    assert_eq!(
        w.call(json!({"op":"frame.begin_rational","dt":ratio(1,999_999_929)}))["ok"],
        false
    );
    assert_eq!(w.snapshot(), before);
}

#[test]
fn fractional_scale_conserves_total_and_remainder_over_many_frames() {
    let w = World::new();
    ok(
        &w,
        json!({"op":"time.set_rational","time_scale":ratio(3,7)}),
    );
    let mut count = 0;
    for _ in 0..12000 {
        count += begin(&w, 1, 60)["fixed_steps"].as_u64().unwrap();
    }
    let exact = ok(&w, json!({"op":"time.get"}))["rational"].clone();
    assert_eq!(count, 4285);
    assert_eq!(exact["time"], ratio(600, 7));
    assert_eq!(exact["admitted_fixed_time"], ratio(857, 10));
    assert_eq!(exact["accumulator"], ratio(1, 70));
    assert_eq!(exact["unscaled_time"], ratio(200, 1));
}

#[test]
fn exact_catchup_limit_accepts_1024_and_rejects_1025_without_mutation() {
    let w = World::new();
    ok(
        &w,
        json!({"op":"time.set_rational","fixed_delta_time":ratio(1,1024)}),
    );
    assert_eq!(begin(&w, 1, 1)["fixed_steps"], 1024);
    ok(
        &w,
        json!({"op":"time.set_rational","fixed_delta_time":ratio(1,1025)}),
    );
    let before = w.snapshot();
    let response = w.call(json!({"op":"frame.begin_rational","dt":ratio(1,1)}));
    assert_eq!(response["ok"], false);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("1024")
    );
    assert_eq!(w.snapshot(), before);
}
