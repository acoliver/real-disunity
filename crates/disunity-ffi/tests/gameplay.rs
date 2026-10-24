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
    fn create(&self, name: &str) -> u64 {
        self.req(json!({"op":"object.create","name":name}))["handle"]
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
fn components_have_distinct_ids_one_owner_and_stale_ids_never_alias() {
    let w = World::new();
    let other = World::new();
    let object = w.create("ball");
    let foreign = other.create("other");
    let a = w.req(json!({"op":"component.add","object":object,"kind":"Rigidbody2D"}));
    let b = w.req(json!({"op":"component.add","object":object,"kind":"CircleCollider2D"}));
    assert_ne!(a["handle"], b["handle"]);
    assert_eq!(a["object"], object);
    assert_eq!(b["object"], object);
    assert_eq!(
        request_value(w.0, json!({"op":"object.get","object":foreign})).unwrap()["ok"],
        false
    );
    w.req(json!({"op":"object.destroy","object":object}));
    let replacement = w.create("new");
    assert_ne!(replacement, object);
    assert_eq!(
        request_value(w.0, json!({"op":"component.get","component":a["handle"]})).unwrap()["ok"],
        false
    );
    assert_eq!(
        request_value(w.0, json!({"op":"object.get","object":object})).unwrap()["ok"],
        false
    );
}

#[test]
fn hierarchy_trs_activity_clone_and_reference_remapping_are_native() {
    let w = World::new();
    let parent = w.create("parent");
    let child = w.create("child");
    w.req(json!({"op":"transform.set","object":parent,"position":[10,0,0],"scale":[2,2,2]}));
    w.req(json!({"op":"transform.parent","object":child,"parent":parent,"world_position_stays":false}));
    w.req(json!({"op":"transform.set","object":child,"position":[1,2,0]}));
    let trs = w.req(json!({"op":"transform.get","object":child}));
    assert_eq!(trs["position"], json!([12.0, 4.0, 0.0]));
    assert_eq!(
        request_value(
            w.0,
            json!({"op":"transform.parent","object":parent,"parent":child})
        )
        .unwrap()["ok"],
        false
    );
    w.req(json!({"op":"component.add","object":parent,"kind":"Script","fields":{"type":"Example","child":{"$ref":child}}}));
    let clone = w.req(json!({"op":"object.clone","object":parent}));
    let cloned_parent = clone["object"]["handle"].as_u64().unwrap();
    let cloned_child = clone["remap"][child.to_string()].as_u64().unwrap();
    let components = w.req(json!({"op":"component.list","object":cloned_parent,"kind":"Script"}));
    assert_eq!(components[0]["fields"]["child"]["$ref"], cloned_child);
    w.req(json!({"op":"object.set","object":parent,"active":false}));
    assert_eq!(
        w.req(json!({"op":"object.get","object":child}))["active_in_hierarchy"],
        false
    );
    assert_eq!(
        w.req(json!({"op":"object.find","name":"child"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    w.req(json!({"op":"object.destroy","object":parent}));
    assert_eq!(
        request_value(w.0, json!({"op":"object.get","object":child})).unwrap()["ok"],
        false
    );
    assert_eq!(
        w.req(json!({"op":"object.get","object":cloned_child}))["active_in_hierarchy"],
        true
    );
}

#[test]
fn old_abi_and_protocol_share_the_same_transform() {
    let w = World::new();
    let object = w.create("shared");
    unsafe {
        assert_eq!(
            disunity_transform_set_position(w.0, object, 3.0, 4.0, 5.0).code,
            0
        );
        assert_eq!(disunity_world_step(w.0, 0.02, 1).code, 0);
    }
    assert_eq!(
        w.req(json!({"op":"transform.get","object":object}))["position"],
        json!([3.0, 4.0, 5.0])
    );
    w.req(json!({"op":"transform.set","object":object,"position":[7,8,9]}));
    let mut output = [worldrunner_disunity_ffi::DisunityObjectState {
        object: 0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        rotation_y_radians: 0.0,
        material_slot: 0,
    }];
    let mut count = 0;
    assert_eq!(
        unsafe { disunity_world_export_objects(w.0, output.as_mut_ptr(), 1, &mut count) }.code,
        0
    );
    assert_eq!([output[0].x, output[0].y, output[0].z], [7.0, 8.0, 9.0]);
}

#[test]
fn legacy_position_commands_use_world_space_for_parented_objects() {
    let w = World::new();
    let parent = w.create("parent");
    let child = w.create("child");
    w.req(json!({"op":"transform.set","object":parent,"position":[10,0,0]}));
    w.req(json!({"op":"transform.parent","object":child,"parent":parent}));
    unsafe {
        assert_eq!(
            disunity_transform_set_position(w.0, child, 1.0, 2.0, 3.0).code,
            0
        );
        assert_eq!(disunity_world_step(w.0, 0.02, 1).code, 0);
    }
    assert_eq!(
        w.req(json!({"op":"transform.get","object":child}))["position"],
        json!([1.0, 2.0, 3.0])
    );
}

#[test]
fn rendering_and_script_metadata_snapshot_and_failed_updates_are_atomic() {
    let w = World::new();
    let object = w.create("visual");
    let sprite=w.req(json!({"op":"component.add","object":object,"kind":"SpriteRenderer","fields":{"sprite":{"guid":"asset","rect":[0,0,16,16]},"color":[1,0.5,0.25,1],"sorting_order":3}}))["handle"].clone();
    w.req(json!({"op":"component.add","object":object,"kind":"Camera","fields":{"orthographic_size":8}}));
    let before = w.req(json!({"op":"snapshot"}));
    assert_eq!(
        before["objects"][0]["components"][1]["fields"]["sprite"]["guid"],
        "asset"
    );
    assert_eq!(request_value(w.0,json!({"op":"component.set","component":sprite,"enabled":false,"fields":{"unsupported":123}})).unwrap()["ok"],false);
    assert_eq!(w.req(json!({"op":"snapshot"})), before);
    assert_eq!(
        request_value(w.0, json!({"version":2,"op":"object.create"})).unwrap()["ok"],
        false
    );
    assert_eq!(w.req(json!({"op":"snapshot"})), before);
}
