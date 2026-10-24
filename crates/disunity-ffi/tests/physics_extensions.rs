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
    fn object(&self, p: [f32; 3]) -> u64 {
        let h = self.req(json!({"op":"object.create"}))["handle"]
            .as_u64()
            .unwrap();
        self.req(json!({"op":"transform.set","object":h,"position":p}));
        h
    }
    fn add(&self, o: u64, kind: &str, fields: Value) -> u64 {
        self.req(json!({"op":"component.add","object":o,"kind":kind,"fields":fields}))["handle"]
            .as_u64()
            .unwrap()
    }
    fn fields(&self, c: u64) -> Value {
        self.req(json!({"op":"component.get","component":c}))["fields"].clone()
    }
    fn position(&self, o: u64) -> Value {
        self.req(json!({"op":"transform.get","object":o}))["position"].clone()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        unsafe {
            disunity_world_destroy(self.0);
        }
    }
}
fn number(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn near(v: f64, expected: f64) {
    assert!((v - expected).abs() < 0.002, "{v} != {expected}");
}

#[test]
fn angular_drag_constraints_and_original_reset_sequence_control_real_motion() {
    let w = World::new();
    let o = w.object([0.0; 3]);
    let b = w.add(
        o,
        "Rigidbody2D",
        json!({"velocity":[2,3],"angular_velocity":180,"angular_drag":2}),
    );
    w.req(json!({"op":"physics.step","dt":0.5}));
    let q = w.req(json!({"op":"transform.get","object":o}))["rotation"].clone();
    near(number(&q[2]), (22.5_f64.to_radians()).sin());
    near(number(&w.fields(b)["angular_velocity"]), 90.0);
    w.req(json!({"op":"component.set","component":b,"fields":{"constraints":5}}));
    let p = w.position(o);
    w.req(json!({"op":"body.move","component":b,"position":[10,4]}));
    w.req(json!({"op":"physics.step","dt":0.5}));
    near(number(&w.position(o)[0]), number(&p[0]));
    near(number(&w.position(o)[1]), 4.0);
    assert_eq!(
        w.req(json!({"op":"transform.get","object":o}))["rotation"],
        q
    );
    w.req(json!({"op":"component.set","component":b,"fields":{"velocity":[0,0],"angular_velocity":0}}));
    w.req(json!({"op":"transform.set","object":o,"position":[0,0,0]}));
    w.req(json!({"op":"body.sleep","component":b}));
    w.req(json!({"op":"physics.step","dt":0.5}));
    assert_eq!(w.position(o), json!([0.0, 0.0, 0.0]));
    w.req(json!({"op":"component.set","component":b,"fields":{"velocity":[0,6]}}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    near(number(&w.position(o)[1]), 0.6);
}

#[test]
fn child_collider_uses_ancestor_motion_and_reports_both_callback_owners() {
    let w = World::new();
    let root = w.object([0.0; 3]);
    let child = w.object([0.0; 3]);
    w.req(
        json!({"op":"transform.parent","object":child,"parent":root,"world_position_stays":false}),
    );
    let body = w.add(
        root,
        "Rigidbody2D",
        json!({"velocity":[100,0],"constraints":4}),
    );
    let c = w.add(child, "BoxCollider2D", json!({"size":[0.32,0.32]}));
    let wall = w.object([5.0, 0.0, 0.0]);
    w.add(wall, "BoxCollider2D", json!({"size":[0.1,10]}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    near(number(&w.fields(body)["velocity"][0]), -100.0);
    assert!(number(&w.position(root)[0]) < 1.0);
    let events = w.req(json!({"op":"events.read"}));
    let e = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["phase"] == "enter")
        .unwrap();
    assert_eq!(e["a"], child);
    assert_eq!(e["body_a"], root);
    assert_eq!(e["rigidbody_a"], body);
    assert_eq!(e["collider_a"], c);
    near(number(&e["point"][0]), 4.95);
    w.req(json!({"op":"component.set","component":body,"fields":{"simulated":false}}));
    assert_eq!(
        w.req(json!({"op":"physics.overlap","point":[number(&w.position(child)[0]),0]})),
        json!([])
    );
}

#[test]
fn original_mass_ratio_slider_bounces_without_drift_or_speed_loss() {
    let w = World::new();
    let paddle = w.object([5.0, 0.0, 0.0]);
    let p = w.add(
        paddle,
        "Rigidbody2D",
        json!({"mass":1000000,"constraints":5}),
    );
    w.add(paddle, "BoxCollider2D", json!({"size":[0.32,2.56]}));
    let ball = w.object([0.0; 3]);
    let b = w.add(
        ball,
        "Rigidbody2D",
        json!({"mass":0.0001,"constraints":4,"velocity":[100,0]}),
    );
    w.add(ball, "BoxCollider2D", json!({"size":[0.32,0.32]}));
    w.req(json!({"op":"body.move","component":p,"position":[6,0.2]}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    near(number(&w.position(paddle)[0]), 5.0);
    near(number(&w.position(paddle)[1]), 0.2);
    near(number(&w.fields(b)["velocity"][0]), -100.0);
    near(number(&w.fields(b)["velocity"][1]), 0.0);
    near(number(&w.fields(p)["velocity"][0]), 0.0);
}

#[test]
fn sleeping_dynamic_body_wakes_on_impact_and_teleport() {
    let w = World::new();
    let a = w.object([0.0; 3]);
    let b = w.object([2.0, 0.0, 0.0]);
    let ba = w.add(a, "Rigidbody2D", json!({"velocity":[10,0],"constraints":4}));
    let bb = w.add(b, "Rigidbody2D", json!({"constraints":4}));
    w.add(a, "CircleCollider2D", json!({}));
    w.add(b, "CircleCollider2D", json!({}));
    w.req(json!({"op":"body.sleep","component":bb}));
    w.req(json!({"op":"physics.step","dt":0.2}));
    near(number(&w.fields(ba)["velocity"][0]), 0.0);
    near(number(&w.fields(bb)["velocity"][0]), 10.0);
    assert_eq!(w.fields(bb)["sleeping"], false);
    near(number(&w.position(b)[0]), 3.0);
    w.req(json!({"op":"body.sleep","component":bb}));
    w.req(json!({"op":"transform.set","object":b,"position":[7,0,0]}));
    assert_eq!(w.fields(bb)["sleeping"], false);
}

#[test]
fn collider_material_overrides_body_and_friction_changes_tangent_motion() {
    for (friction, expected_y) in [(0.0, 4.0), (1.0, 0.0)] {
        let w = World::new();
        let a = w.object([0.0; 3]);
        let b = w.object([2.0, 0.0, 0.0]);
        let body = w.add(
            a,
            "Rigidbody2D",
            json!({"velocity":[10,4],"constraints":4,"restitution":1}),
        );
        w.add(
            a,
            "BoxCollider2D",
            json!({"material":{"bounciness":0.5,"friction":friction,"asset":{"guid":"elastic"}}}),
        );
        w.add(
            b,
            "BoxCollider2D",
            json!({"size":[1,10],"material":{"bounciness":0,"friction":friction}}),
        );
        w.req(json!({"op":"physics.step","dt":0.2}));
        near(number(&w.fields(body)["velocity"][0]), -5.0);
        near(number(&w.fields(body)["velocity"][1]), expected_y);
        assert!(number(&w.position(a)[0]) < 1.0);
    }
}

#[test]
fn layers_filter_contacts_and_queries_independently_and_reject_bad_masks_atomically() {
    let w = World::new();
    let a = w.object([0.0; 3]);
    let b = w.object([2.0, 0.0, 0.0]);
    w.req(json!({"op":"object.set","object":a,"layer":8,"tag":"Ball"}));
    w.req(json!({"op":"object.set","object":b,"layer":9}));
    w.add(a, "Rigidbody2D", json!({"velocity":[10,0],"constraints":4}));
    w.add(a, "CircleCollider2D", json!({"radius":0.25}));
    w.add(b, "BoxCollider2D", json!({"size":[0.1,10]}));
    w.req(json!({"op":"physics.layers","layer_a":8,"layer_b":9,"ignore":true}));
    w.req(json!({"op":"physics.step","dt":0.3}));
    near(number(&w.position(a)[0]), 3.0);
    assert_eq!(w.req(json!({"op":"events.read"})), json!([]));
    let hits = w.req(json!({"op":"physics.raycast","origin":[0,0],"direction":[1,0],"distance":5,"layer_mask":512}));
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["object"], b);
    assert_eq!(
        w.req(json!({"op":"physics.overlap","point":[2,0],"layer_mask":256})),
        json!([])
    );
    let before = w.req(json!({"op":"object.get","object":a}));
    assert_eq!(
        request_value(
            w.0,
            json!({"op":"object.set","object":a,"layer":32,"tag":"bad"})
        )
        .unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"object.get","object":a})), before);
}
