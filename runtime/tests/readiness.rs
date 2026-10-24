use std::path::PathBuf;

use serde_json::{Value, json};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

fn config() -> ManagedHostConfig {
    let root = PathBuf::from(std::env::var_os("RD_WORKTREE").unwrap());
    let managed = PathBuf::from(std::env::var_os("DISUNITY_TEST_MANAGED_ROOT").unwrap());
    let assembly = managed.join("artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll");
    ManagedHostConfig {
        ffi_library: root.join(".work/resumed-native-readiness/provider.so"),
        runtime_config: assembly.with_extension("runtimeconfig.json"),
        assembly,
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn fixture(host: &CompatibilityHost, world: u64, kind: &str, fields: Value) -> Value {
    let object = host
        .request(
            world,
            &json!({"op":"object.create","name":"runtime-readiness"}),
        )
        .unwrap()["handle"]
        .clone();
    let script = host
        .request(
            world,
            &json!({"op":"component.add","object":object,
        "kind":"Script","fields":{"type":kind}}),
        )
        .unwrap()["handle"]
        .clone();
    let managed = PathBuf::from(std::env::var_os("DISUNITY_TEST_MANAGED_ROOT").unwrap());
    json!({"world":world,
        "assemblies":[managed.join("artifacts/bin/Compatibility.Tests/debug/Compatibility.Tests.dll")],
        "requirements":{"execution_profile":"native_phase_context_v1","scheduler_profile":"native_readiness_single_v1"},
        "handles":{"owner":object,"script":script},
        "graph":{"nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
        "components":[{"id":"script","object":"owner","script":{"type_name":kind,"fields":fields}}]}})
}

fn fields(host: &CompatibilityHost) -> Value {
    host.observe(&json!({"op":"managed.snapshot"})).unwrap()["data"]["instances"][0]["fields"]
        .clone()
}

fn assert_drained(host: &CompatibilityHost) {
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for name in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][name], 0);
    }
}

#[test]
fn actual_runtime_cdecl_native_readiness_opt_in_and_failure_continuation() {
    let config = config();
    // The native image has TLS users that outlive individual host instances.
    let image = unsafe { libloading::Library::new(&config.ffi_library).unwrap() };
    std::mem::forget(image);
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    let manifest = fixture(&host, world, "ReadinessProbe", json!({}));
    host.open_phase_session(world).unwrap();
    reject_profiles(&mut host, world, &manifest);
    host.load_fixture(&manifest)
        .expect("explicit bounded scheduler must negotiate");
    assert_eq!(fields(&host)["awakes"], 1);
    host.advance_rational(world, 0, 1).unwrap();
    host.advance_rational(world, 0, 1).unwrap();
    assert_eq!(fields(&host)["starts"], 1);
    assert_eq!(fields(&host)["moves"], 2);
    assert_eq!(fields(&host)["disposals"], 1);
    host.unload().unwrap();
    assert_drained(&host);
    assert_eq!(
        host.request(world, &json!({"op":"session.status"}))
            .unwrap()["stage"],
        "Closed"
    );
    host.destroy_world(world).unwrap();
    unsupported_is_application_failure(&mut host);
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn unsupported_is_application_failure(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    let manifest = fixture(host, world, "ReadinessWaitProbe", json!({"mode":8}));
    host.open_phase_session(world).unwrap();
    host.load_fixture(&manifest).unwrap();
    assert!(host.advance_rational(world, 0, 1).is_err());
    assert_eq!(host.callback_failures(0).unwrap().total, 1);
    assert_eq!(fields(host)["disposals"], 1);
    assert_eq!(
        host.request(world, &json!({"op":"session.status"}))
            .unwrap()["stage"],
        "Idle"
    );
    assert!(host.advance_rational(world, 0, 1).is_err());
    assert_eq!(host.callback_failures(0).unwrap().total, 1);
    host.unload().unwrap();
    assert_drained(host);
    host.destroy_world(world).unwrap();
}

fn reject_profiles(host: &mut CompatibilityHost, world: u64, manifest: &Value) {
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    for profile in [
        Value::Null,
        json!(1),
        json!({}),
        json!("native_scheduler_v1"),
    ] {
        let mut bad = manifest.clone();
        bad["requirements"]["scheduler_profile"] = profile;
        assert!(host.load_fixture(&bad).is_err());
        assert_drained(host);
        assert_eq!(
            host.request(world, &json!({"op":"session.status"}))
                .unwrap(),
            before
        );
    }
    let mut bad = manifest.clone();
    bad["requirements"]
        .as_object_mut()
        .unwrap()
        .remove("execution_profile");
    assert!(host.load_fixture(&bad).is_err());
    assert_drained(host);
}
