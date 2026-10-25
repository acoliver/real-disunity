use serde_json::{Value, json};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

pub fn config() -> ManagedHostConfig {
    crate::support::artifacts().config()
}

pub fn fixture(host: &CompatibilityHost, world: u64, mode: u32, depth: u32) -> Value {
    let root = crate::support::artifacts().managed_root.clone();
    let object = host
        .request(world, &json!({"op":"object.create","name":"nested-host"}))
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"NestedScenarioProbe"}})).unwrap()["handle"].as_u64().unwrap();
    json!({"world":world,"assemblies":[root.join("artifacts/bin/Compatibility.Tests/debug/Compatibility.Tests.dll")],
        "requirements":{"execution_profile":"native_phase_context_v1","scheduler_profile":"native_readiness_nested_start_v1",
        "transport_profile":"transport_byte_limits_v1","native_capabilities":["native_terminal_cleanup_v1"],"bridge_capabilities":["managed_terminal_cleanup_v1"]},
        "handles":{"owner":object,"probe":script},"graph":{"nodes":[{"id":"owner","active":true,"template":false}],
        "components":[{"id":"probe","object":"owner","script":{"type_name":"NestedScenarioProbe","fields":{"mode":mode,"depth":depth}}}],"assets":[],"aliases":{}}})
}

pub fn add_peer(host: &CompatibilityHost, world: u64, manifest: &mut Value) {
    let object = host
        .request(
            world,
            &json!({"op":"object.create","name":"nested-budget-peer"}),
        )
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"ReadinessProbe"}})).unwrap()["handle"].as_u64().unwrap();
    manifest["handles"]["peer-owner"] = json!(object);
    manifest["handles"]["peer"] = json!(script);
    manifest["graph"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"peer-owner","active":true,"template":false}));
    manifest["graph"]["components"].as_array_mut().unwrap().push(json!({"id":"peer","object":"peer-owner","script":{"type_name":"ReadinessProbe","fields":{}}}));
}
