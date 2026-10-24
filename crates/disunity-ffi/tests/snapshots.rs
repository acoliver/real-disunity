use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value, response::*};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { disunity_world_create(&mut handle) }.code, 0);
        Self(handle)
    }
    fn value(&self, request: Value) -> Value {
        let response = request_value(self.0, request).unwrap();
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }
    fn bytes(&self, request: Value) -> Value {
        let handle = request_bytes(self.0, &serde_json::to_vec(&request).unwrap()).unwrap();
        let bytes = response_bytes(handle).unwrap();
        response_free(handle).unwrap();
        serde_json::from_slice::<Value>(&bytes).unwrap()["data"].clone()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn snapshots_match_individual_queries_and_preserve_nested_component_fields() {
    let w = World::new();
    let parent = w.value(json!({"op":"object.create","name":"parent"}))["handle"]
        .as_u64()
        .unwrap();
    let child = w.value(json!({"op":"object.create","name":"child"}))["handle"]
        .as_u64()
        .unwrap();
    w.value(json!({"op":"transform.set","object":parent,"position":[3,4,0]}));
    w.value(json!({"op":"transform.parent","object":child,"parent":parent,"world_position_stays":false}));
    w.value(json!({"op":"transform.set","object":child,"position":[1,2,0]}));
    let fields =
        json!({"type":"Example","nested":{"array":[null,true,7,1.25,{"text":"original"}]}});
    let component = w.value(json!({"op":"component.add","object":child,"kind":"Script","fields":fields}))["handle"].as_u64().unwrap();
    w.value(json!({"op":"object.set","object":parent,"active":false}));
    let before = w.value(json!({"op":"snapshot"}));
    assert_eq!(before, w.bytes(json!({"op":"snapshot"})));
    assert_eq!(before["objects"].as_array().unwrap().len(), 2);
    for (index, object) in [parent, child].into_iter().enumerate() {
        let snapshot = &before["objects"][index];
        assert_eq!(
            *snapshot,
            w.value(json!({"op":"object.get","object":object}))
        );
        assert_eq!(
            snapshot["components"],
            w.value(json!({"op":"component.list","object":object}))
        );
        assert_eq!(
            snapshot["transform"],
            w.value(json!({"op":"transform.get","object":object}))
        );
        assert_eq!(snapshot["active_in_hierarchy"], false);
    }
    assert_eq!(
        before["objects"][1]["transform"]["position"],
        json!([4.0, 6.0, 0.0])
    );
    assert_eq!(before["objects"][1]["components"][1]["fields"], fields);
    w.value(json!({"op":"component.set","component":component,"fields":{"nested":{"array":["changed"]}}}));
    let after = w.bytes(json!({"op":"snapshot"}));
    assert_eq!(before["objects"][1]["components"][1]["fields"], fields);
    assert_eq!(
        after["objects"][1]["components"][1]["fields"]["nested"]["array"],
        json!(["changed"])
    );
    assert_eq!(after["time"], w.value(json!({"op":"time.get"})));
    assert_eq!(after["input"], w.value(json!({"op":"input.get"})));
}
