#[path = "support/handshake.rs"]
pub mod support;

use serde_json::{Value, json};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

fn config() -> ManagedHostConfig {
    crate::support::artifacts().config()
}

fn manifest(host: &CompatibilityHost, world: u64) -> Value {
    let root = crate::support::artifacts().managed_root.clone();
    let object = host
        .request(world, &json!({"op":"object.create","name":"byte-probe"}))
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"StrictPhaseProbe"}})).unwrap()["handle"].as_u64().unwrap();
    let assemblies: Vec<_> = ["Compatibility.Tests", "Snake", "Breakout", "Pong"]
        .into_iter()
        .map(|name| root.join(format!("artifacts/bin/{name}/debug/{name}.dll")))
        .collect();
    json!({"world":world,"assemblies":assemblies,
        "requirements":{"execution_profile":"native_phase_context_v1"},
        "handles":{"owner":object,"script":script},
        "graph":{"nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
        "components":[{"id":"script","object":"owner","script":{"type_name":"StrictPhaseProbe","fields":{"failUpdate":false}}}]}})
}

#[test]
fn actual_cdecl_byte_host_matches_before_initialize_and_loads_original_pes() {
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    let world = host.create_world().unwrap();

    host.open_phase_session(world).unwrap();
    let manifest = manifest(&host, world);
    host.load_fixture(&manifest).unwrap();
    let observation = host.observe(&json!({"op":"managed.snapshot"})).unwrap();
    assert_eq!(observation["data"]["scripts"], 1);
    host.advance_rational(world, 0, 1).unwrap();
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}
