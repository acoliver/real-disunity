use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value};
struct World(u64);
impl World {
    fn new() -> Self {
        let mut h = 0;
        assert_eq!(unsafe { disunity_world_create(&mut h) }.code, 0);
        Self(h)
    }
    fn req(&self, v: Value) -> Value {
        let r = request_value(self.0, v).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        r["data"].clone()
    }
    fn configure(&self) {
        self.req(json!({"op":"input.configure_axes","axes":{
            "Vertical":{"negative_keys":["S"],"positive_keys":["W"],"sensitivity":3,"gravity":3,"dead":0.001,"snap":true},
            "RightPaddle":{"negative_keys":["DownArrow"],"positive_keys":["UpArrow"]}
        }}));
    }
}
impl Drop for World {
    fn drop(&mut self) {
        unsafe {
            disunity_world_destroy(self.0);
        }
    }
}

#[test]
fn source_keyboard_axes_and_edges_are_native_and_reads_are_stable() {
    let w = World::new();
    w.configure();
    let held = w.req(json!({"op":"input.set","keys":["W","DownArrow"]}));
    assert_eq!(held["axes"]["Vertical"], 1.0);
    assert_eq!(held["axes"]["RightPaddle"], -1.0);
    assert_eq!(held["down"], json!(["DownArrow", "W"]));
    for _ in 0..3 {
        assert_eq!(w.req(json!({"op":"input.get"})), held);
    }
    let next = w.req(json!({"op":"input.set","keys":["W","DownArrow"]}));
    assert_eq!(next["down"], json!([]));
    assert_eq!(next["axes"], held["axes"]);
    let opposite = w.req(json!({"op":"input.set","keys":["W","S","UpArrow"]}));
    assert_eq!(opposite["axes"]["Vertical"], 0.0);
    assert_eq!(opposite["axes"]["RightPaddle"], 1.0);
    assert_eq!(opposite["up"], json!(["DownArrow"]));
}

#[test]
fn synthetic_axes_override_keys_for_one_frame_and_invalid_configuration_is_atomic() {
    let w = World::new();
    w.configure();
    assert_eq!(
        w.req(json!({"op":"input.set","keys":["W"],"axes":{"Vertical":0.25}}))["axes"]["Vertical"],
        0.25
    );
    assert_eq!(
        w.req(json!({"op":"input.set","keys":["W"]}))["axes"]["Vertical"],
        1.0
    );
    let before = w.req(json!({"op":"input.get"}));
    assert_eq!(
        request_value(
            w.0,
            json!({"op":"input.configure_axes","axes":{"Vertical":{"dead":-1}}})
        )
        .unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"input.get"})), before);
    assert_eq!(
        w.req(json!({"op":"input.set","keys":["S"]}))["axes"]["Vertical"],
        -1.0
    );
}

#[test]
fn focus_loss_releases_held_keys_and_axes_until_focus_returns() {
    let w = World::new();
    w.configure();
    w.req(json!({"op":"input.set","keys":["W"]}));
    let released = w.req(json!({"op":"input.focus","focused":false}));
    assert_eq!(released["keys"], json!([]));
    assert_eq!(released["up"], json!(["W"]));
    assert_eq!(released["axes"]["Vertical"], 0.0);
    assert_eq!(w.req(json!({"op":"input.get"})), released);
    let ignored = w.req(json!({"op":"input.set","keys":["W"],"axes":{"Vertical":1}}));
    assert_eq!(ignored["axes"]["Vertical"], 0.0);
    assert_eq!(ignored["down"], json!([]));
    w.req(json!({"op":"input.focus","focused":true}));
    assert_eq!(
        w.req(json!({"op":"input.set","keys":["W"]}))["down"],
        json!(["W"])
    );
}
