use std::path::PathBuf;

use serde_json::{Value, json};

use super::CompatibilityHost;
use crate::ManagedHostConfig;

mod controls;
mod fault;

fn config() -> Option<(ManagedHostConfig, PathBuf)> {
    let root = std::env::var_os("DISUNITY_TERMINAL_MANAGED_ROOT")?;
    let root = PathBuf::from(root).join("artifacts/bin");
    let bridge = root.join("CompatibilityBridge/debug/CompatibilityBridge.dll");
    Some((
        ManagedHostConfig {
            ffi_library: std::env::var_os("DISUNITY_TEST_NATIVE_LIBRARY")
                .unwrap()
                .into(),
            runtime_config: bridge.with_extension("runtimeconfig.json"),
            assembly: bridge,
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        },
        root.join("Compatibility.Tests/debug/Compatibility.Tests.dll"),
    ))
}

fn fixture(host: &CompatibilityHost, world: u64, assembly: &PathBuf) -> (Value, Vec<u64>) {
    let mut handles = serde_json::Map::new();
    let mut nodes = Vec::new();
    let mut components = Vec::new();
    let mut objects = Vec::new();
    for index in 0..2 {
        let id = format!("owner{index}");
        let script_id = format!("script{index}");
        let object = host
            .request(world, &json!({"op":"object.create","name":id}))
            .unwrap()["handle"]
            .as_u64()
            .unwrap();
        let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"TerminalCleanupProbe"}})).unwrap()["handle"].as_u64().unwrap();
        objects.push(object);
        handles.insert(id.clone(), json!(object));
        handles.insert(script_id.clone(), json!(script));
        nodes.push(json!({"id":id,"active":true,"template":false}));
        components.push(json!({"id":script_id,"object":id,"script":{"type_name":"TerminalCleanupProbe","fields":{"failCleanup":index == 0}}}));
    }
    let mut manifest = json!({"world":world,"assemblies":[assembly],"requirements":{"execution_profile":"native_phase_context_v1","native_capabilities":["native_terminal_cleanup_v1"],"bridge_capabilities":["managed_terminal_cleanup_v1"]},"handles":handles,"graph":{"nodes":nodes,"components":components,"assets":[],"aliases":{}}});
    if std::env::var_os("DISUNITY_TERMINAL_READINESS").is_some() {
        manifest["requirements"]["scheduler_profile"] = json!("native_readiness_single_v1");
    }
    (manifest, objects)
}

#[test]
fn actual_cdecl_fatal_transport_after_fixed_drains_roots() {
    let Some((config, assembly)) = config() else {
        return;
    };
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.request(world, &json!({"op":"input.set","keys":["W"]}))
        .unwrap();
    host.request(world, &json!({"op":"random.seed","seed":12345}))
        .unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, objects) = fixture(&host, world, &assembly);
    host.load_fixture(&manifest).unwrap();
    fault::install(&host);
    fault::PHASE.set(host.bridge.phase).unwrap();
    host.bridge.phase = fault::after_fixed;
    let error = host
        .advance_rational(world, 3, 100)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("-77"),
        "primary transport failure lost: {error}"
    );
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    eprintln!("actual Cdecl injected transport -77; measured roots={roots}");
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0, "fatal cleanup retained {key}");
    }
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["stage"], "Closed");
    assert_eq!(status["frame"], 1);
    assert_eq!(status["fixed_tick"], 0);
    assert_eq!(
        fault::state(&host, world),
        fault::ACCEPTED.lock().unwrap().clone().unwrap()
    );
    for object in objects {
        let transform = host
            .request(world, &json!({"op":"transform.get","object":object}))
            .unwrap();
        eprintln!("post-cleanup mutation-bearing transform={transform}");
        assert_eq!(transform["position"], json!([1., 11., 10.]));
        assert_eq!(transform["scale"][0], 1.);
        for axis in [1, 2] {
            let value = transform["scale"][axis].as_f64().unwrap();
            assert!((0.0..1_000_000.0).contains(&value));
        }
    }
    assert_eq!(host.callback_failures(0).unwrap().total, 3);
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn actual_cdecl_partial_binding_retains_owned_roots_before_cleanup() {
    let Some((config, assembly)) = config() else {
        return;
    };
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (mut manifest, _) = fixture(&host, world, &assembly);
    manifest["graph"]["components"][1]["script"]["type_name"] = json!("MissingType");
    let context = host.phase_context(world, "Initialize").unwrap();
    manifest["initial_context"] = context;
    host.strict.get_mut(&world).unwrap().cleanup = super::terminal::CleanupState::Ready;
    host.loaded = true;
    host.loaded_world = Some(world);
    let error = host.bridge.invoke(host.bridge.load, &manifest).unwrap_err();
    assert!(error.to_string().contains("script type mismatch"));
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    eprintln!("actual partial binding error={error}; measured roots={roots}");
    assert_eq!(
        roots["data"]["scripts"], 1,
        "partial session ownership was lost"
    );
    assert!(roots["data"]["bindings"].as_u64().unwrap() > 0);
    assert_eq!(host.loaded_world, Some(world));
    assert!(host.load_fixture(&manifest).is_err());
    host.unload().unwrap();
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0);
    }
    host.destroy_world(world).unwrap();
}
