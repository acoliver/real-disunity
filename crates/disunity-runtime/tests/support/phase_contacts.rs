use serde_json::{Value, json};
use worldrunner_disunity_runtime::CompatibilityHost;

pub fn numeric_runtime_frame(host: &mut CompatibilityHost, assembly: &std::path::Path) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, peer) = fixture(host, world, assembly);
    for (id, y) in [("left", 0.0), ("right", 0.05)] {
        let object = &manifest["handles"][id];
        host.request(
            world,
            &json!({"op":"transform.set","object":object,
            "position":[-0.36050745844841003,y,0.0]}),
        )
        .unwrap();
        let colliders = host
            .request(
                world,
                &json!({"op":"component.list",
            "object":object,"kind":"BoxCollider2D"}),
            )
            .unwrap();
        host.request(
            world,
            &json!({"op":"component.set","component":colliders[0]["handle"],
            "fields":{"size":[0.125,0.125]}}),
        )
        .unwrap();
    }
    let bodies = host
        .request(
            world,
            &json!({"op":"component.list",
        "object":peer,"kind":"Rigidbody2D"}),
        )
        .unwrap();
    host.request(
        world,
        &json!({"op":"component.set","component":bodies[0]["handle"],
        "fields":{"velocity":[0.0,-0.25]}}),
    )
    .unwrap();
    host.load_fixture(&manifest).unwrap();
    let error = host.advance_rational(world, 1, 50).unwrap_err().to_string();
    assert!(error.contains("solver contact sentinel"), "{error}");
    delivered_frame(host, world, peer);
    host.unload().unwrap();
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0);
    }
    host.destroy_world(world).unwrap();
}

pub fn runtime_frame(host: &mut CompatibilityHost, assembly: &std::path::Path) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, peer) = fixture(host, world, assembly);
    host.load_fixture(&manifest).unwrap();
    let error = host.advance_rational(world, 1, 50).unwrap_err().to_string();
    assert!(error.contains("solver contact sentinel"), "{error}");
    let status = delivered_frame(host, world, peer);
    host.unload().unwrap();
    let after = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(after["stage"], "Closed");
    assert_eq!(after["frame"], status["frame"]);
    assert_eq!(after["fixed_tick"], status["fixed_tick"]);
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0);
    }
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn delivered_frame(host: &CompatibilityHost, world: u64, peer: u64) -> Value {
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":peer}))
            .unwrap()["position"][2],
        1.
    );
    assert_eq!(
        host.request(world, &json!({"op":"events.read","clear":false}))
            .unwrap(),
        json!([])
    );
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["stage"], "Idle");
    assert_eq!(status["frame"], 1);
    assert_eq!(status["fixed_tick"], 1);
    assert!(status["active_phase"].is_null());
    let failures = host.callback_failures(0).unwrap();
    assert_eq!(failures.total, 1);
    assert_eq!(failures.records[0].phase, "Contacts");
    assert_eq!(failures.records[0].frame, Some(1));
    status
}

pub fn fixture(host: &CompatibilityHost, world: u64, assembly: &std::path::Path) -> (Value, u64) {
    let mut handles = serde_json::Map::new();
    let mut nodes = Vec::new();
    let mut components = Vec::new();
    let mut peer = 0;
    for (id, fail) in [("left", true), ("right", false)] {
        let object = host
            .request(world, &json!({"op":"object.create","name":id}))
            .unwrap()["handle"]
            .as_u64()
            .unwrap();
        let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"IsolatedContactProbe"}})).unwrap()["handle"].as_u64().unwrap();
        handles.insert(id.into(), json!(object));
        handles.insert(format!("{id}-script"), json!(script));
        nodes.push(json!({"id":id,"template":false,"active":true}));
        components.push(json!({"id":format!("{id}-script"),"object":id,"script":{"type_name":"IsolatedContactProbe","fields":{"fail":fail}}}));
        host.request(world, &json!({"op":"component.add","object":object,"kind":"BoxCollider2D","fields":{"is_trigger":true}})).unwrap();
        if !fail {
            peer = object;
            host.request(world, &json!({"op":"component.add","object":object,"kind":"Rigidbody2D","fields":{"gravity_scale":0}})).unwrap();
        }
    }
    (
        json!({"world":world,"requirements":{"execution_profile":"native_phase_context_v1"},"assemblies":[assembly],"handles":handles,
        "graph":{"nodes":nodes,"components":components,"assets":[],"aliases":{}}}),
        peer,
    )
}
