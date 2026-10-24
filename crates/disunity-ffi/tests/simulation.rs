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
    fn create(&self, n: &str) -> u64 {
        self.req(json!({"op":"object.create","name":n}))["handle"]
            .as_u64()
            .unwrap()
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
fn native_clock_input_edges_and_random_sequence_are_observable() {
    let w = World::new();
    w.req(json!({"op":"input.set","keys":["W"],"axes":{"Vertical":1.0}}));
    let first = w.req(json!({"op":"input.get"}));
    assert_eq!(first["down"], json!(["W"]));
    assert_eq!(w.req(json!({"op":"input.get"})), first);
    w.req(json!({"op":"input.set","keys":["W"],"axes":{}}));
    assert_eq!(w.req(json!({"op":"input.get"}))["down"], json!([]));
    w.req(json!({"op":"input.set","keys":[],"axes":{}}));
    assert_eq!(w.req(json!({"op":"input.get"}))["up"], json!(["W"]));
    w.req(json!({"op":"time.set","time_scale":0.5}));
    let time = w.req(json!({"op":"frame.begin","dt":0.2}));
    assert!((time["time"].as_f64().unwrap() - 0.1).abs() < 1e-6);
    assert_eq!(time["frame"], 1);
    w.req(json!({"op":"random.seed","seed":42}));
    let a = w.req(json!({"op":"random.range","min":0,"max":100,"integer":true}));
    w.req(json!({"op":"random.seed","seed":42}));
    assert_eq!(
        w.req(json!({"op":"random.range","min":0,"max":100,"integer":true})),
        a
    );
    assert!((0..100).contains(&a.as_i64().unwrap()));
}
#[test]
fn continuous_native_physics_bounces_fast_ball_and_reports_trigger_crossing() {
    let w = World::new();
    let ball = w.create("ball");
    let wall = w.create("wall");
    let trigger = w.create("trigger");
    let body=w.req(json!({"op":"component.add","object":ball,"kind":"Rigidbody2D","fields":{"velocity":[100,0]}}))["handle"].clone();
    w.req(json!({"op":"component.add","object":ball,"kind":"CircleCollider2D","fields":{"radius":0.25}}));
    w.req(json!({"op":"transform.set","object":wall,"position":[5,0,0]}));
    w.req(json!({"op":"component.add","object":wall,"kind":"BoxCollider2D","fields":{"size":[0.1,10]}}));
    w.req(json!({"op":"transform.set","object":trigger,"position":[2,0,0]}));
    w.req(json!({"op":"component.add","object":trigger,"kind":"BoxCollider2D","fields":{"size":[0.1,10],"is_trigger":true}}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    let velocity = w.req(json!({"op":"component.get","component":body}))["fields"]["velocity"][0]
        .as_f64()
        .unwrap();
    assert!(velocity < -99.0, "{velocity}");
    let pos = w.req(json!({"op":"transform.get","object":ball}))["position"][0]
        .as_f64()
        .unwrap();
    assert!(pos < 1.0, "ball tunneled through wall: {pos}");
    let events = w.req(json!({"op":"events.read"}));
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["trigger"] == true && e["phase"] == "enter")
    );
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["trigger"] == false && e["phase"] == "enter")
    );
    assert!(
        w.req(json!({"op":"events.read"}))
            .as_array()
            .unwrap()
            .is_empty()
    );
    let hits =
        w.req(json!({"op":"physics.raycast","origin":[0,0],"direction":[1,0],"distance":10}));
    assert!(hits.as_array().unwrap().iter().any(|h| h["object"] == wall));
}

#[test]
fn move_position_is_native_swept_motion_and_sleep_stops_integration() {
    let w = World::new();
    let paddle = w.create("paddle");
    let body=w.req(json!({"op":"component.add","object":paddle,"kind":"Rigidbody2D","fields":{"body_type":"kinematic"}}))["handle"].clone();
    w.req(json!({"op":"body.move","component":body,"position":[2,3]}));
    assert_eq!(
        w.req(json!({"op":"transform.get","object":paddle}))["position"],
        json!([0.0, 0.0, 0.0])
    );
    w.req(json!({"op":"physics.step","dt":0.02}));
    let p = w.req(json!({"op":"transform.get","object":paddle}))["position"].clone();
    assert!((p[0].as_f64().unwrap() - 2.0).abs() < 1e-5);
    assert!((p[1].as_f64().unwrap() - 3.0).abs() < 1e-5);
    w.req(json!({"op":"physics.step","dt":0.02}));
    assert_eq!(
        w.req(json!({"op":"transform.get","object":paddle}))["position"],
        p
    );
    w.req(json!({"op":"component.set","component":body,"fields":{"body_type":"dynamic","velocity":[1,0]}}));
    w.req(json!({"op":"body.sleep","component":body}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    assert_eq!(
        w.req(json!({"op":"transform.get","object":paddle}))["position"],
        p
    );
    w.req(json!({"op":"body.force","component":body,"force":[2,0],"mode":"impulse"}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    assert!(
        w.req(json!({"op":"transform.get","object":paddle}))["position"][0]
            .as_f64()
            .unwrap()
            > p[0].as_f64().unwrap() + 0.1
    );
}

#[test]
fn trigger_contact_lifecycle_and_disabled_colliders_are_native() {
    let w = World::new();
    let a = w.create("a");
    let b = w.create("b");
    w.req(json!({"op":"component.add","object":a,"kind":"Rigidbody2D"}));
    w.req(json!({"op":"component.add","object":a,"kind":"CircleCollider2D","fields":{"is_trigger":true}}));
    let collider =
        w.req(json!({"op":"component.add","object":b,"kind":"CircleCollider2D"}))["handle"].clone();
    w.req(json!({"op":"physics.step","dt":0.02}));
    assert_eq!(w.req(json!({"op":"events.read"}))[0]["phase"], "enter");
    w.req(json!({"op":"physics.step","dt":0.02}));
    assert_eq!(w.req(json!({"op":"events.read"}))[0]["phase"], "stay");
    w.req(json!({"op":"component.set","component":collider,"enabled":false}));
    w.req(json!({"op":"physics.step","dt":0.02}));
    assert_eq!(w.req(json!({"op":"events.read"}))[0]["phase"], "exit");
    assert_eq!(
        w.req(json!({"op":"physics.overlap","point":[0,0]}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn polygon_and_box_sweeps_preserve_elastic_speed_without_tunneling() {
    for kind in ["BoxCollider2D", "PolygonCollider2D"] {
        let w = World::new();
        let ball = w.create("moving");
        let wall = w.create("wall");
        let body=w.req(json!({"op":"component.add","object":ball,"kind":"Rigidbody2D","fields":{"velocity":[100,0]}}))["handle"].clone();
        let fields = if kind == "PolygonCollider2D" {
            json!({"points":[[-0.2,-0.2],[0.2,-0.2],[0.2,0.2],[-0.2,0.2]]})
        } else {
            json!({"size":[0.4,0.4]})
        };
        w.req(json!({"op":"component.add","object":ball,"kind":kind,"fields":fields}));
        w.req(json!({"op":"transform.set","object":wall,"position":[5,0,0]}));
        w.req(json!({"op":"component.add","object":wall,"kind":"BoxCollider2D","fields":{"size":[0.1,10]}}));
        w.req(json!({"op":"physics.step","dt":0.1}));
        assert!(
            w.req(json!({"op":"transform.get","object":ball}))["position"][0]
                .as_f64()
                .unwrap()
                < 1.0
        );
        assert!(
            (w.req(json!({"op":"component.get","component":body}))["fields"]["velocity"][0]
                .as_f64()
                .unwrap()
                + 100.0)
                .abs()
                < 0.001
        );
    }
}
