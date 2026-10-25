use serde_json::{Map, Value, json};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

pub fn config() -> ManagedHostConfig {
    crate::support::artifacts().config()
}

pub fn manifest(host: &CompatibilityHost, world: u64, script_type: &str, count: u64) -> Value {
    let owner = host
        .request(
            world,
            &json!({"op":"object.create","name":"rust-contextual-owner"}),
        )
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let mut handles = Map::from_iter([("owner".into(), json!(owner))]);
    let mut components = Vec::new();
    for role in 0..count {
        let id = format!("probe{role}");
        let component = host.request(world, &json!({"op":"component.add","object":owner,"kind":"Script","fields":{"type":script_type}})).unwrap()["handle"].clone();
        handles.insert(id.clone(), component);
        let fields = if script_type == "ContextualCleanupProbe" {
            json!({"role":role})
        } else {
            json!({})
        };
        components.push(
            json!({"id":id,"object":"owner","script":{"type_name":script_type,"fields":fields}}),
        );
    }
    let managed = crate::support::artifacts().managed_root.clone();
    json!({"world":world,"assemblies":[managed.join("artifacts/bin/Compatibility.Tests/debug/Compatibility.Tests.dll")],
        "requirements":{"execution_profile":"native_phase_context_v1","scheduler_profile":"native_contextual_lifecycle_batch_v1",
            "callback_profile":"contextual_lifecycle_batch_v1","native_capabilities":["native_contextual_lifecycle_batch_v1"],
            "bridge_capabilities":["managed_contextual_cleanup_batch_consumer_v1"]},
        "handles":handles,"graph":{"nodes":[{"id":"owner","template":false,"active":true}],
            "assets":[],"aliases":{},"components":components}})
}
