use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { disunity_world_create(&mut handle) }.code, 0);
        Self(handle)
    }
    fn response(&self, request: Value) -> Value {
        request_value(self.0, request).unwrap()
    }
    fn req(&self, request: Value) -> Value {
        let response = self.response(request);
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }
    fn ball(&self) -> (u64, u64) {
        let object = self.req(json!({"op":"object.create","name":"Ball"}))["handle"]
            .as_u64()
            .unwrap();
        let component = self.req(
            json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"Ball"}}),
        )["handle"]
            .as_u64()
            .unwrap();
        (object, component)
    }
    fn count(&self) -> usize {
        self.req(json!({"op":"object.find","kind":"Ball"}))
            .as_array()
            .unwrap()
            .len()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn frame_destruction_preserves_source_find_visibility_then_retires_generations() {
    let world = World::new();
    let (lost, collider) = world.ball();
    let (survivor, _) = world.ball();
    world.req(json!({"op":"lifetime.begin"}));
    world.req(json!({"op":"object.destroy","object":lost}));
    assert_eq!(
        world.count(),
        2,
        "DeathZone must still count the pending ball"
    );
    assert_eq!(
        world.req(json!({"op":"component.get","component":collider}))["object"],
        lost
    );
    world.req(json!({"op":"lifetime.end"}));
    assert_eq!(world.count(), 1);
    assert_eq!(
        world.response(json!({"op":"object.get","object":lost}))["ok"],
        false
    );
    assert_eq!(
        world.req(json!({"op":"object.get","object":survivor}))["handle"],
        survivor
    );
    assert_ne!(world.ball().0, lost);
}

#[test]
fn pending_ancestor_and_component_removal_are_idempotent_and_do_not_touch_reuse() {
    let world = World::new();
    let (parent, script) = world.ball();
    let (child, _) = world.ball();
    world.req(json!({"op":"transform.parent","object":child,"parent":parent}));
    world.req(json!({"op":"lifetime.begin"}));
    world.req(json!({"op":"object.destroy","object":parent}));
    world.req(json!({"op":"object.destroy","object":parent}));
    world.req(json!({"op":"component.remove","component":script}));
    world.req(json!({"op":"object.destroy","object":child}));
    assert_eq!(world.count(), 2);
    world.req(json!({"op":"lifetime.end"}));
    assert_eq!(world.count(), 0);
    let (replacement, _) = world.ball();
    assert_ne!(replacement, parent);
    assert_ne!(replacement, child);
    assert_eq!(
        world.response(json!({"op":"object.destroy","object":parent}))["ok"],
        false
    );
    assert_eq!(world.count(), 1);
}

#[test]
fn scope_errors_are_transactional_and_legacy_destruction_stays_immediate() {
    let world = World::new();
    let (ball, _) = world.ball();
    assert_eq!(world.response(json!({"op":"lifetime.end"}))["ok"], false);
    world.req(json!({"op":"lifetime.begin"}));
    assert_eq!(world.response(json!({"op":"lifetime.begin"}))["ok"], false);
    assert_eq!(
        world.response(json!({"op":"lifetime.end","unknown":true}))["ok"],
        false
    );
    world.req(json!({"op":"object.destroy","object":ball}));
    assert_eq!(world.count(), 1);
    world.req(json!({"op":"lifetime.end"}));
    let (next, _) = world.ball();
    world.req(json!({"op":"object.destroy","object":next}));
    assert_eq!(world.count(), 0);
}
