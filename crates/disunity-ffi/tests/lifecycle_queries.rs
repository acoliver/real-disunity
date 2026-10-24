use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { disunity_world_create(&mut handle) }.code, 0);
        Self(handle)
    }
    fn req(&self, value: Value) -> Value {
        let response = request_value(self.0, value).unwrap();
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }
    fn create(&self) -> u64 {
        self.req(json!({"op":"object.create"}))["handle"]
            .as_u64()
            .unwrap()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn lifecycle_projection_matches_live_hierarchy_script_enable_and_generations() {
    let world = World::new();
    let other = World::new();
    let parent = world.create();
    let child = world.create();
    world.req(json!({"op":"transform.parent","object":child,"parent":parent}));
    let script = world.req(json!({"op":"component.add","object":child,"kind":"Script","fields":{"type":"Probe","fields":{"value":7}}}))["handle"].as_u64().unwrap();
    let snapshot = world.req(json!({"op":"lifecycle.snapshot"}));
    assert_eq!(
        snapshot["scripts"],
        json!([{"handle":script,"object":child,"active_in_hierarchy":true,"enabled":true}])
    );
    assert!(
        snapshot["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["handle"] == script && c["object"] == child)
    );
    assert_eq!(world.req(json!({"op":"object.alive","object":child})), true);
    assert_eq!(
        world.req(json!({"op":"component.alive","component":script})),
        true
    );
    assert_eq!(
        other.req(json!({"op":"object.alive","object":child})),
        false
    );
    world.req(json!({"op":"object.set","object":parent,"active":false}));
    world.req(json!({"op":"component.set","component":script,"enabled":false}));
    let snapshot = world.req(json!({"op":"lifecycle.snapshot"}));
    assert_eq!(snapshot["scripts"][0]["active_in_hierarchy"], false);
    assert_eq!(snapshot["scripts"][0]["enabled"], false);
    world.req(json!({"op":"object.destroy","object":parent}));
    assert_eq!(
        world.req(json!({"op":"object.alive","object":child})),
        false
    );
    assert_eq!(
        world.req(json!({"op":"component.alive","component":script})),
        false
    );
    assert!(
        world.req(json!({"op":"lifecycle.snapshot"}))["scripts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_ne!(world.create(), child);
    assert_eq!(
        world.req(json!({"op":"object.alive","object":child})),
        false
    );
    assert_eq!(
        request_value(world.0, json!({"op":"lifecycle.snapshot","unknown":1})).unwrap()["ok"],
        false
    );
}
