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
    fn object(&self, position: [f32; 2]) -> u64 {
        let handle = self.req(json!({"op":"object.create"}))["handle"]
            .as_u64()
            .unwrap();
        self.req(
            json!({"op":"transform.set","object":handle,"position":[position[0],position[1],0]}),
        );
        handle
    }
    fn add(&self, object: u64, kind: &str, fields: Value) -> u64 {
        self.req(json!({"op":"component.add","object":object,"kind":kind,"fields":fields}))["handle"]
            .as_u64()
            .unwrap()
    }
    fn body(&self, object: u64, kind: &str) -> u64 {
        self.add(
            object,
            "Rigidbody2D",
            json!({"body_type":kind,"constraints":4}),
        )
    }
    fn fields(&self, component: u64) -> Value {
        self.req(json!({"op":"component.get","component":component}))["fields"].clone()
    }
    fn step(&self, dt: f32) -> Vec<Value> {
        self.req(json!({"op":"physics.step","dt":dt}));
        self.req(json!({"op":"events.read"}))
            .as_array()
            .unwrap()
            .clone()
    }
    fn parent(&self, child: u64, parent: u64) {
        self.req(json!({"op":"transform.parent","object":child,"parent":parent,"world_position_stays":false}));
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn adjacent_bodyless_shapes_have_no_contacts_and_remain_queryable() {
    let w = World::new();
    let a = w.object([0.0, 0.0]);
    let b = w.object([1.0, 0.0]);
    w.add(a, "BoxCollider2D", json!({}));
    w.add(b, "BoxCollider2D", json!({}));
    let before = w.req(json!({"op":"snapshot"}));
    assert_eq!(w.step(0.02), Vec::<Value>::new());
    assert_eq!(w.step(0.02), Vec::<Value>::new());
    let overlap = w.req(json!({"op":"physics.overlap","point":[0.5,0]}));
    assert_eq!(overlap.as_array().unwrap().len(), 2);
    let ray = w.req(json!({"op":"physics.raycast","origin":[-2,0],"direction":[1,0],"distance":5}));
    assert_eq!(ray.as_array().unwrap().len(), 2);
    assert_eq!(
        w.req(json!({"op":"snapshot"}))["objects"],
        before["objects"]
    );
}

fn pair_events(a: Option<&str>, b: Option<&str>, trigger_side: Option<usize>) -> Vec<Value> {
    let w = World::new();
    for (i, kind) in [a, b].into_iter().enumerate() {
        let object = w.object([0.0, 0.0]);
        if let Some(kind) = kind {
            w.body(object, kind);
        }
        w.add(
            object,
            "BoxCollider2D",
            json!({"is_trigger":trigger_side == Some(i)}),
        );
    }
    w.step(0.0)
}

#[test]
fn normal_contact_matrix_requires_a_dynamic_body() {
    let kinds = [None, Some("static"), Some("kinematic"), Some("dynamic")];
    let expected = [
        [false, false, false, true],
        [false, false, false, true],
        [false, false, false, true],
        [true, true, true, true],
    ];
    for (i, a) in kinds.into_iter().enumerate() {
        for (j, b) in kinds.into_iter().enumerate() {
            let events = pair_events(a, b, None);
            assert_eq!(
                events.len(),
                usize::from(expected[i][j]),
                "{a:?}/{b:?}: {events:?}"
            );
            assert!(
                events
                    .iter()
                    .all(|e| e["phase"] == "enter" && e["trigger"] == false)
            );
        }
    }
}

#[test]
fn trigger_matrix_requires_a_dynamic_or_kinematic_body_on_either_side() {
    let kinds = [None, Some("static"), Some("kinematic"), Some("dynamic")];
    let expected = [
        [false, false, true, true],
        [false, false, true, true],
        [true, true, true, true],
        [true, true, true, true],
    ];
    for side in [0, 1] {
        for (i, a) in kinds.into_iter().enumerate() {
            for (j, b) in kinds.into_iter().enumerate() {
                let events = pair_events(a, b, Some(side));
                assert_eq!(
                    events.len(),
                    usize::from(expected[i][j]),
                    "{a:?}/{b:?}, trigger {side}: {events:?}"
                );
                assert!(
                    events
                        .iter()
                        .all(|e| e["phase"] == "enter" && e["trigger"] == true)
                );
            }
        }
    }
}

#[test]
fn kinematic_sweeps_ignore_non_dynamic_solids_but_detect_triggers() {
    for other in [None, Some("static"), Some("kinematic")] {
        for trigger in [false, true] {
            let w = World::new();
            let a = w.object([0.0, 0.0]);
            let body = w.body(a, "kinematic");
            w.add(a, "BoxCollider2D", json!({"size":[0.5,0.5]}));
            let b = w.object([2.0, 0.0]);
            if let Some(kind) = other {
                w.body(b, kind);
            }
            w.add(b, "BoxCollider2D", json!({"is_trigger":trigger}));
            w.req(json!({"op":"body.move","component":body,"position":[4,0]}));
            let events = w.step(0.1);
            assert_eq!(
                events.len(),
                if trigger { 2 } else { 0 },
                "{other:?}: {events:?}"
            );
            assert_eq!(
                w.req(json!({"op":"transform.get","object":a}))["position"],
                json!([4.0, 0.0, 0.0])
            );
            if trigger {
                assert_eq!(events[0]["phase"], "enter");
                assert_eq!(events[1]["phase"], "exit");
            }
        }
    }
}

#[test]
fn dynamic_body_bounces_from_a_kinematic_body_and_preserves_callback_identity() {
    let w = World::new();
    let a = w.object([0.0, 0.0]);
    let body_a = w.body(a, "dynamic");
    w.req(json!({"op":"component.set","component":body_a,"fields":{"velocity":[10,0]}}));
    let collider_a = w.add(a, "CircleCollider2D", json!({"radius":0.25}));
    let b = w.object([2.0, 0.0]);
    let body_b = w.body(b, "kinematic");
    let collider_b = w.add(b, "BoxCollider2D", json!({"size":[0.5,2]}));
    let events = w.step(0.3);
    assert!(w.fields(body_a)["velocity"][0].as_f64().unwrap() < -9.99);
    assert_eq!(w.fields(body_b)["velocity"], json!([0.0, 0.0]));
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["phase"], "enter");
    assert_eq!(events[0]["rigidbody_a"], body_a);
    assert_eq!(events[0]["rigidbody_b"], body_b);
    assert_eq!(events[0]["collider_a"], collider_a);
    assert_eq!(events[0]["collider_b"], collider_b);
}

#[test]
fn compound_colliders_do_not_contact_their_attached_body_or_siblings() {
    for kind in ["dynamic", "kinematic"] {
        let w = World::new();
        let root = w.object([0.0, 0.0]);
        let body = w.body(root, kind);
        w.add(root, "BoxCollider2D", json!({}));
        for trigger in [false, true] {
            let child = w.object([0.0, 0.0]);
            w.parent(child, root);
            w.add(child, "CircleCollider2D", json!({"is_trigger":trigger}));
        }
        assert!(w.step(0.02).is_empty());
        let hits = w.req(json!({"op":"physics.overlap","point":[0,0]}));
        assert_eq!(hits.as_array().unwrap().len(), 3);
        assert!(
            hits.as_array()
                .unwrap()
                .iter()
                .all(|h| h["attached_body"] == body)
        );
    }
}

#[test]
fn nearest_ancestor_body_type_controls_child_contacts() {
    let w = World::new();
    let root = w.object([0.0, 0.0]);
    w.body(root, "dynamic");
    let child = w.object([0.0, 0.0]);
    w.parent(child, root);
    let child_body = w.body(child, "kinematic");
    let leaf = w.object([0.0, 0.0]);
    w.parent(leaf, child);
    w.add(leaf, "CircleCollider2D", json!({}));
    let other = w.object([0.0, 0.0]);
    w.add(other, "BoxCollider2D", json!({}));
    assert!(w.step(0.0).is_empty());
    w.req(json!({"op":"component.set","component":child_body,"fields":{"body_type":"dynamic"}}));
    let events = w.step(0.0);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["a"], leaf);
    assert_eq!(events[0]["body_a"], child);
    assert_eq!(events[0]["rigidbody_a"], child_body);
}

#[test]
fn sleeping_and_fully_constrained_dynamic_bodies_still_generate_contacts() {
    let w = World::new();
    let a = w.object([0.0, 0.0]);
    let body = w.body(a, "dynamic");
    w.req(json!({"op":"component.set","component":body,"fields":{"constraints":7}}));
    w.add(a, "BoxCollider2D", json!({}));
    let b = w.object([0.0, 0.0]);
    w.add(b, "BoxCollider2D", json!({}));
    w.req(json!({"op":"body.sleep","component":body}));
    let events = w.step(0.02);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["phase"], "enter");
    assert_eq!(w.fields(body)["sleeping"], true);
    assert_eq!(w.step(0.02)[0]["phase"], "stay");
}

#[test]
fn body_type_simulation_and_layer_changes_end_and_restore_trigger_contacts() {
    let w = World::new();
    let a = w.object([0.0, 0.0]);
    let body = w.body(a, "dynamic");
    w.add(a, "BoxCollider2D", json!({"is_trigger":true}));
    let b = w.object([0.0, 0.0]);
    w.add(b, "BoxCollider2D", json!({}));
    assert_eq!(w.step(0.02)[0]["phase"], "enter");
    for fields in [json!({"body_type":"static"}), json!({"simulated":false})] {
        w.req(json!({"op":"component.set","component":body,"fields":fields}));
        assert_eq!(w.step(0.02)[0]["phase"], "exit");
        assert!(w.step(0.02).is_empty());
        w.req(json!({"op":"component.set","component":body,"fields":{"body_type":"dynamic","simulated":true}}));
        assert_eq!(w.step(0.02)[0]["phase"], "enter");
    }
    w.req(json!({"op":"physics.layers","layer_a":0,"layer_b":0,"ignore":true}));
    assert_eq!(w.step(0.02)[0]["phase"], "exit");
    assert!(w.step(0.02).is_empty());
    assert_eq!(
        w.req(json!({"op":"physics.overlap","point":[0,0]}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    w.req(json!({"op":"physics.layers","layer_a":0,"layer_b":0,"ignore":false}));
    assert_eq!(w.step(0.02)[0]["phase"], "enter");
}

#[test]
fn unsupported_full_kinematic_contacts_is_a_transactional_field_error() {
    let w = World::new();
    let object = w.object([0.0, 0.0]);
    let body = w.body(object, "kinematic");
    let before = w.fields(body);
    let error = request_value(
        w.0,
        json!({"op":"component.set","component":body,"fields":{"full_kinematic_contacts":true}}),
    )
    .unwrap();
    assert_eq!(error["ok"], false);
    assert!(error.to_string().contains("unknown field"), "{error}");
    assert_eq!(w.fields(body), before);
}
