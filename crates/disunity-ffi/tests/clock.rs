use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { disunity_world_create(&mut handle) }.code, 0);
        Self(handle)
    }
    fn req(&self, request: Value) -> Value {
        let response = request_value(self.0, request).unwrap();
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        unsafe {
            disunity_world_destroy(self.0);
        }
    }
}
fn near(value: &Value, expected: f64) {
    assert!(
        (value.as_f64().unwrap() - expected).abs() < 1e-6,
        "{value} != {expected}"
    );
}

#[test]
fn six_render_frames_admit_five_native_fixed_steps() {
    let w = World::new();
    let mut steps = 0;
    for _ in 0..6 {
        let frame = w.req(json!({"op":"frame.begin","dt":1.0 / 60.0}));
        steps += frame["fixed_steps"]
            .as_u64()
            .expect("native fixed admission");
        near(&frame["fixed_delta_time"], 0.02);
    }
    assert_eq!(steps, 5);
    let time = w.req(json!({"op":"time.get"}));
    near(&time["time"], 0.1);
    assert_eq!(time["frame"], 6);
    assert_eq!(time["fixed_frame"], 0);
}

#[test]
fn phase_switches_delta_without_advancing_clock_or_physics() {
    let w = World::new();
    w.req(json!({"op":"time.set","time_scale":0.5}));
    let frame = w.req(json!({"op":"frame.begin","dt":0.1}));
    assert_eq!(frame["fixed_steps"], 2);
    let fixed = w.req(json!({"op":"time.phase","phase":"fixed"}));
    near(&fixed["delta_time"], 0.02);
    near(&fixed["time"], 0.05);
    assert_eq!(fixed["frame"], 1);
    assert_eq!(fixed["fixed_frame"], 0);
    w.req(json!({"op":"physics.step"}));
    let update = w.req(json!({"op":"time.phase","phase":"update"}));
    near(&update["delta_time"], 0.05);
    near(&update["time"], 0.05);
    assert_eq!(update["fixed_frame"], 1);
    let before = w.req(json!({"op":"time.get"}));
    assert_eq!(
        request_value(w.0, json!({"op":"time.phase","phase":"late"})).unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"time.get"})), before);
}

#[test]
fn pause_preserves_remainder_and_scale_controls_admission() {
    let w = World::new();
    assert_eq!(
        w.req(json!({"op":"frame.begin","dt":0.01}))["fixed_steps"],
        0
    );
    w.req(json!({"op":"time.set","time_scale":0}));
    let paused = w.req(json!({"op":"frame.begin","dt":1}));
    assert_eq!(paused["fixed_steps"], 0);
    near(&paused["delta_time"], 0.0);
    near(&paused["time"], 0.01);
    w.req(json!({"op":"time.set","time_scale":2}));
    assert_eq!(
        w.req(json!({"op":"frame.begin","dt":0.015}))["fixed_steps"],
        2
    );
}

#[test]
fn invalid_delta_and_excessive_admission_leave_clock_unchanged() {
    let w = World::new();
    w.req(json!({"op":"frame.begin","dt":0.01}));
    for dt in [-0.01, 1.01] {
        let before = w.req(json!({"op":"time.get"}));
        assert_eq!(
            request_value(w.0, json!({"op":"frame.begin","dt":dt})).unwrap()["ok"],
            false
        );
        assert_eq!(w.req(json!({"op":"time.get"})), before);
    }
    w.req(json!({"op":"time.set","fixed_delta_time":0.0001}));
    let before = w.req(json!({"op":"time.get"}));
    assert_eq!(
        request_value(w.0, json!({"op":"frame.begin","dt":1})).unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"time.get"})), before);
    w.req(json!({"op":"time.set","fixed_delta_time":0.02}));
    assert_eq!(
        w.req(json!({"op":"frame.begin","dt":0.01}))["fixed_steps"],
        1
    );
}

#[test]
fn legacy_step_does_not_accrue_fixed_admission_debt() {
    let w = World::new();
    w.req(json!({"op":"time.set","fixed_delta_time":0.000001}));
    assert_eq!(unsafe { disunity_world_step(w.0, 0.5, 1) }.code, 0);
    let legacy = w.req(json!({"op":"time.get"}));
    near(&legacy["time"], 0.5);
    assert_eq!(legacy["frame"], 1);
    assert_eq!(legacy["fixed_frame"], 1);
    assert_eq!(w.req(json!({"op":"frame.begin","dt":0}))["fixed_steps"], 0);
}
