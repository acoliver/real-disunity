#[path = "support/handshake.rs"]
pub mod support;

use serde_json::{Value, json};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

fn config() -> ManagedHostConfig {
    let artifacts = support::artifacts();
    ManagedHostConfig {
        ffi_library: artifacts.native.clone(),
        runtime_config: artifacts.bridge().with_extension("runtimeconfig.json"),
        assembly: artifacts.bridge(),
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn fixture(host: &CompatibilityHost, world: u64, kind: &str) -> (Value, u64) {
    let assembly = &support::artifacts().probe();
    let mut nodes = Vec::new();
    let mut components = Vec::new();
    let mut handles = serde_json::Map::new();
    let mut peer = 0;
    for (id, fields) in [
        ("failure", json!({"failure":"Update"})),
        ("peer", json!({})),
    ] {
        let object = host
            .request(world, &json!({"op":"object.create","name":id}))
            .unwrap()["handle"]
            .as_u64()
            .unwrap();
        let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":kind}})).unwrap()["handle"].as_u64().unwrap();
        handles.insert(id.into(), json!(object));
        handles.insert(format!("{id}-script"), json!(script));
        nodes.push(json!({"id":id,"template":false,"active":true}));
        components.push(json!({"id":format!("{id}-script"),"object":id,"script":{"type_name":kind,"fields":fields}}));
        if id == "peer" {
            peer = object;
        }
    }
    (
        json!({"world":world,"assemblies":[assembly],"handles":handles,"graph":{"nodes":nodes,"components":components,"assets":[],"aliases":{}}}),
        peer,
    )
}

#[test]
fn actual_cdecl_callback_failure_continues_peer_and_fails_host_gate() {
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    let world = host.create_world().unwrap();
    let (manifest, peer) = fixture(&host, world, "CallbackIsolationProbe");
    host.load_fixture(&manifest).unwrap();
    let error = host
        .dispatch_phase(&json!({"phase":"Update"}))
        .unwrap_err()
        .to_string();
    let position = host
        .request(world, &json!({"op":"transform.get","object":peer}))
        .unwrap();
    assert_eq!(
        position["position"],
        json!([1., 1., 1.]),
        "eligible native peer must initialize/start despite earlier failure"
    );
    assert_eq!(
        position["scale"],
        json!([1., 2., 1.]),
        "peer Update must execute after earlier callback throws"
    );
    assert!(
        error.contains("callback sentinel Update") && error.contains("System.ArgumentException"),
        "wrong error: {error}"
    );
    assert_failure_observation(&host);
    assert!(
        host.dispatch_phase(&json!({"phase":"LateUpdate"})).is_err(),
        "unconsumed failures cannot silently turn green"
    );
    host.unload().unwrap();
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["scripts"], 0);
    assert_eq!(roots["data"]["bindings"], 0);
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    initialization_and_solver_gates(&mut host);
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn assert_failure_observation(host: &CompatibilityHost) {
    let query = json!({"op":"managed.failures"});
    let failures = host.observe(&query).unwrap();
    assert_eq!(
        failures,
        host.observe(&query).unwrap(),
        "observations must retain failures"
    );
    assert_eq!(failures["data"]["total"], 1);
    let record = &failures["data"]["records"][0];
    assert_eq!(record["phase"], "Update");
    assert_eq!(record["exception_type"], "System.ArgumentException");
    assert_eq!(record["callback"], "Update");
    assert!(
        record["stack"]
            .as_str()
            .unwrap()
            .contains("CallbackIsolationProbe.Update")
    );
    assert!(record["frame"].is_null());
}

fn initialization_and_solver_gates(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    let (mut manifest, peer) = fixture(host, world, "CallbackIsolationProbe");
    manifest["graph"]["components"][0]["script"]["fields"] = json!({"failure":"Awake"});
    let error = host.load_fixture(&manifest).unwrap_err().to_string();
    assert!(error.contains("callback sentinel Awake"));
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":peer}))
            .unwrap()["position"],
        json!([1., 1., 0.])
    );
    let page = host.callback_failures(0).unwrap();
    assert_eq!(page.records[0].phase, "Initialize");
    assert_eq!(page.records[0].exception_type, "System.ArgumentException");
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    solver_gate(host);
}

fn solver_gate(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    let (mut manifest, peer) = fixture(host, world, "IsolatedContactProbe");
    let first = manifest["handles"]["failure"].as_u64().unwrap();
    for (index, fail) in [(0, true), (1, false)] {
        manifest["graph"]["components"][index]["script"]["type_name"] =
            json!("IsolatedContactProbe");
        manifest["graph"]["components"][index]["script"]["fields"] = json!({"fail":fail});
    }
    for (object, kind, fields) in [
        (first, "BoxCollider2D", json!({"is_trigger":true})),
        (peer, "CircleCollider2D", json!({})),
        (peer, "Rigidbody2D", json!({"gravity_scale":0})),
    ] {
        host.request(
            world,
            &json!({"op":"component.add","object":object,"kind":kind,"fields":fields}),
        )
        .unwrap();
    }
    host.load_fixture(&manifest).unwrap();
    host.request(world, &json!({"op":"physics.step","dt":0.02}))
        .unwrap();
    let events = host.request(world, &json!({"op":"events.read"})).unwrap();
    let enter = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["phase"] == "enter")
        .expect("real native contact required");
    let error = host.invoke_contact(enter).unwrap_err().to_string();
    assert!(error.contains("solver contact sentinel"));
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":peer}))
            .unwrap()["position"][2],
        1.
    );
    assert_eq!(
        host.callback_failures(0).unwrap().records[0].phase,
        "contacts.enter"
    );
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
}
