#[path = "support/handshake.rs"]
mod support;

use serde_json::json;
use worldrunner_disunity_runtime::{CompatibilityHost, HostError, ManagedHostConfig};

fn config() -> ManagedHostConfig {
    let artifacts = support::artifacts();
    ManagedHostConfig {
        ffi_library: artifacts.native.clone(),
        runtime_config: artifacts.bridge().with_extension("runtimeconfig.json"),
        assembly: artifacts.bridge(),
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn actual_host_rejects_unnegotiated_provider_before_world_allocation() {
    for _ in 0..2 {
        let result = CompatibilityHost::boot(&config());
        let error = result
            .err()
            .expect("boot accepted a provider without a compatible pure descriptor");
        eprintln!("provider rejected: {error}");
        assert!(matches!(
            error,
            HostError::Abi(_)
                | HostError::Request(_)
                | HostError::Compatibility(_)
                | HostError::ResponseBudget(_)
        ));
    }
}

fn actual_host_validates_created_and_existing_world_before_fixture_callbacks() {
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    assert_eq!(host.store_statistics().unwrap(), (0, 0));
    let world = host.create_world().unwrap();
    let before = host.request(world, &json!({"op":"snapshot"})).unwrap();
    for bad in [
        json!({"world":world,"version":2}),
        json!({"world":world,"schema_version":2}),
        json!({"world":world,"requirements":{"pointer_width":32}}),
        json!({"world":world,"requirements":{"native_abi_version":99}}),
        json!({"world":world,"requirements":{"native_capabilities":["phase_tokens"]}}),
        json!({"world":0}),
        json!({"world":1}),
    ] {
        let error = host.load_fixture(&bad).unwrap_err().to_string();
        assert!(
            error.contains("handshake") || error.contains("ABI"),
            "wrong preactivation error: {error}"
        );
        assert_eq!(
            host.request(world, &json!({"op":"snapshot"})).unwrap(),
            before
        );
    }
    activate_probe(&mut host, world);
    host.destroy_world(world).unwrap();
    assert!(host.load_fixture(&json!({"world":world})).is_err());
    external_world(&mut host);
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn created_world_handshake_failure_destroys_the_fresh_world() {
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    assert!(host.create_world().is_err());
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn activate_probe(host: &mut CompatibilityHost, world: u64) {
    let assembly = &support::artifacts().probe();
    let object = host
        .request(
            world,
            &json!({"op":"object.create","name":"host-handshake"}),
        )
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let transform = host
        .request(
            world,
            &json!({"op":"component.list","object":object,"kind":"Transform"}),
        )
        .unwrap()[0]["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"HandshakeProbe"}})).unwrap()["handle"].as_u64().unwrap();
    host.load_fixture(&json!({"version":1,"schema_version":1,"world":world,"assemblies":[assembly],
        "requirements":{"native_abi_version":2,"native_capabilities":["describe"],"bridge_capabilities":["describe_bridge"]},
        "handles":{"owner":object,"transform":transform,"script":script},"graph":{
            "nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
            "components":[{"id":"transform","object":"owner","script":null},
            {"id":"script","object":"owner","script":{"type_name":"HandshakeProbe","fields":{}}}]}})).unwrap();
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap()["position"],
        json!([99., 0., 0.])
    );
    host.unload().unwrap();
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["scripts"], 0);
    assert_eq!(roots["data"]["bindings"], 0);
}

#[test]
fn actual_cdecl_handshake_acceptance() {
    if std::env::var_os("HANDSHAKE_EXPECT_REJECT").is_some() {
        actual_host_rejects_unnegotiated_provider_before_world_allocation();
    } else if std::env::var_os("HANDSHAKE_WORLD_REJECT").is_some() {
        created_world_handshake_failure_destroys_the_fresh_world();
    } else {
        actual_host_validates_created_and_existing_world_before_fixture_callbacks();
    }
}

fn external_world(host: &mut CompatibilityHost) {
    // SAFETY: this is the same trusted image retained by the host, with the ABI 2 signature.
    let library = unsafe { libloading::Library::new(config().ffi_library).unwrap() };
    let create: unsafe extern "C" fn(*mut u64) -> NativeResult =
        unsafe { *library.get(b"disunity_world_create\0").unwrap() };
    let mut world = 0;
    assert_eq!(unsafe { create(&mut world) }.code, 0);
    activate_probe(host, world);
    host.destroy_world(world).unwrap();
}

#[repr(C)]
struct NativeResult {
    code: i32,
    detail: u64,
}
