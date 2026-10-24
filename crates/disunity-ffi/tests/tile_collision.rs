use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, request_value};

struct World(u64);
impl World {
    fn new() -> Self {
        let mut h = 0;
        assert_eq!(unsafe { disunity_world_create(&mut h) }.code, 0);
        Self(h)
    }
    fn req(&self, request: Value) -> Value {
        let r = request_value(self.0, request).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        r["data"].clone()
    }
    fn create(&self) -> u64 {
        self.req(json!({"op":"object.create"}))["handle"]
            .as_u64()
            .unwrap()
    }
    fn add(&self, object: u64, kind: &str, fields: Value) -> u64 {
        self.req(json!({"op":"component.add","object":object,"kind":kind,"fields":fields}))["handle"].as_u64().unwrap()
    }
    fn overlap(&self, point: [f32; 2]) -> Value {
        self.req(json!({"op":"physics.overlap","point":point}))
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn grid_cells_respect_size_gap_parent_pose_and_clone_ownership() {
    let w = World::new();
    let grid = w.create();
    w.req(json!({"op":"transform.set","object":grid,"position":[10,2,0],"scale":[2,1,1]}));
    let tiles = w.create();
    w.req(
        json!({"op":"transform.parent","object":tiles,"parent":grid,"world_position_stays":false}),
    );
    let collider = w.add(tiles, "TilemapCollider2D", json!({"cell_size":[2,1],"cell_gap":[1,0],"cells":[{"position":[0,0],"collider_type":"grid"},{"position":[1,0],"collider_type":"grid"},{"position":[2,0],"collider_type":"none"}]}));
    assert_eq!(w.overlap([12.0, 2.5])[0]["handle"], collider);
    assert_eq!(w.overlap([15.0, 2.5]), json!([]));
    assert_eq!(w.overlap([18.0, 2.5])[0]["object"], tiles);
    assert_eq!(w.overlap([24.0, 2.5]), json!([]));
    let cloned = w.req(json!({"op":"object.clone","object":grid}));
    let clone_root = cloned["object"]["handle"].as_u64().unwrap();
    let clone_tiles = cloned["remap"][tiles.to_string()].as_u64().unwrap();
    let clone_collider = cloned["remap"][collider.to_string()].as_u64().unwrap();
    w.req(json!({"op":"transform.set","object":clone_root,"position":[-10,2,0]}));
    assert_eq!(w.overlap([-8.0, 2.5])[0]["handle"], clone_collider);
    assert_eq!(w.overlap([-8.0, 2.5])[0]["object"], clone_tiles);
    w.req(json!({"op":"object.destroy","object":grid}));
    assert_eq!(w.overlap([12.0, 2.5]), json!([]));
    assert_eq!(w.overlap([-8.0, 2.5])[0]["handle"], clone_collider);
}

#[test]
fn sprite_polygons_anchor_affine_offset_and_queries_use_one_component_identity() {
    let w = World::new();
    let tiles = w.create();
    let collider = w.add(tiles, "TilemapCollider2D", json!({"cell_size":[2,2],"tile_anchor":[0.25,0.5],"offset":[3,-2],"cells":[{"position":[0,0],"collider_type":"sprite","sprite_size":[2,2],"matrix":[0,1,-1,0,1,0],"physics_shapes":[[[0,0],[1,0],[0,1]]]},{"position":[0,0],"collider_type":"sprite","sprite_size":[2,2],"matrix":[0,1,-1,0,1,0],"physics_shapes":[[[0,0],[1,0],[0,1]]]}]}));
    let inside = w.overlap([4.3, -0.8]);
    assert_eq!(inside.as_array().unwrap().len(), 1);
    assert_eq!(inside[0]["handle"], collider);
    assert_eq!(w.overlap([3.7, -0.2]), json!([]));
    let hits =
        w.req(json!({"op":"physics.raycast","origin":[2,-0.8],"direction":[1,0],"distance":10}));
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["collider"], collider);
    assert!((hits[0]["distance"].as_f64().unwrap() - 1.7).abs() < 0.001);
}

#[test]
fn fast_ball_bounces_from_tile_geometry_and_events_identify_original_collider() {
    let w = World::new();
    let tiles = w.create();
    let wall = w.add(tiles,"TilemapCollider2D",json!({"cell_size":[1,1],"cells":[{"position":[4,0],"collider_type":"grid"},{"position":[4,1],"collider_type":"grid"}],"material":{"bounciness":1,"friction":0}}));
    let ball = w.create();
    w.req(json!({"op":"transform.set","object":ball,"position":[0,0.5,0]}));
    let body = w.add(
        ball,
        "Rigidbody2D",
        json!({"velocity":[100,0],"constraints":4}),
    );
    let circle = w.add(ball, "CircleCollider2D", json!({"radius":0.25}));
    w.req(json!({"op":"physics.step","dt":0.1}));
    let velocity = w.req(json!({"op":"component.get","component":body}))["fields"]["velocity"][0]
        .as_f64()
        .unwrap();
    assert!(velocity < -99.0, "{velocity}");
    let events = w.req(json!({"op":"events.read"}));
    let enter = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["phase"] == "enter")
        .unwrap();
    assert_eq!(enter["collider_a"], wall);
    assert_eq!(enter["collider_b"], circle);
    assert_eq!(enter["a"], tiles);
    assert_eq!(enter["body_b"], ball);
}

#[test]
fn tile_updates_are_transactional_and_disabled_or_trigger_geometry_obeys_queries() {
    let w = World::new();
    let tiles = w.create();
    let c = w.add(
        tiles,
        "TilemapCollider2D",
        json!({"is_trigger":true,"cells":[{"position":[0,0],"collider_type":"grid"}]}),
    );
    let before = w.req(json!({"op":"component.get","component":c}));
    let error = request_value(
        w.0,
        json!({"op":"component.set","component":c,"fields":{"cell_size":[0,1]}}),
    )
    .unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(w.req(json!({"op":"component.get","component":c})), before);
    assert_eq!(
        w.req(json!({"op":"physics.overlap","point":[0.5,0.5],"include_triggers":false})),
        json!([])
    );
    assert_eq!(w.overlap([0.5, 0.5])[0]["handle"], c);
    w.req(json!({"op":"component.set","component":c,"enabled":false}));
    assert_eq!(w.overlap([0.5, 0.5]), json!([]));
}

#[test]
fn replacing_cells_rebuilds_collision_geometry_and_invalid_or_oversized_edits_roll_back() {
    let w = World::new();
    let object = w.create();
    let collider = w.add(
        object,
        "TilemapCollider2D",
        json!({"cells":[{"position":[0,0]}]}),
    );
    w.req(json!({"op":"component.set","component":collider,"fields":{"cells":[{"position":[2,3],"collider_type":"sprite","sprite_size":[2,1]}]}}));
    assert_eq!(w.overlap([0.5, 0.5]), json!([]));
    assert_eq!(w.overlap([2.0, 3.5])[0]["handle"], collider);
    let before = w.req(json!({"op":"component.get","component":collider}));
    for fields in [
        json!({"cells":[{"collider_type":"sprite"}]}),
        json!({"cells":[{"matrix":[0,0,0,0,0,0]}]}),
        json!({"cells":[{"collider_type":"sprite","physics_shapes":[[[0,0],[1,0],[0.2,0.2],[0,1]]]}]}),
        json!({"cells":vec![json!({});4097]}),
        json!({"unsupported":true}),
    ] {
        let error = request_value(
            w.0,
            json!({"op":"component.set","component":collider,"fields":fields}),
        )
        .unwrap();
        assert_eq!(error["ok"], false, "{error}");
        assert_eq!(
            w.req(json!({"op":"component.get","component":collider})),
            before
        );
        assert_eq!(w.overlap([2.0, 3.5])[0]["handle"], collider);
    }
}
