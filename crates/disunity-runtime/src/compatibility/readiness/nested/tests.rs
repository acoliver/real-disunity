use std::path::PathBuf;

use serde_json::{Value, json};

use crate::compatibility::{CompatibilityHost, transport::NativeTransport};

#[path = "fault.rs"]
mod fault;

fn fixture(host: &CompatibilityHost, world: u64, assembly: &PathBuf) -> (Value, u64) {
    let object = host
        .request(world, &json!({"op":"object.create","name":"nested-fatal"}))
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"NestedFatalProbe"}})).unwrap()["handle"].as_u64().unwrap();
    (
        json!({"world":world,"assemblies":[assembly],"requirements":{"execution_profile":"native_phase_context_v1",
        "scheduler_profile":"native_readiness_nested_start_v1","native_capabilities":["native_terminal_cleanup_v1"],"bridge_capabilities":["managed_terminal_cleanup_v1"]},
        "handles":{"owner":object,"probe":script},"graph":{"nodes":[{"id":"owner","active":true,"template":false}],
        "components":[{"id":"probe","object":"owner","script":{"type_name":"NestedFatalProbe","fields":{}}}],"assets":[],"aliases":{}}}),
        object,
    )
}

#[test]
fn actual_cdecl_nested_fatal_after_unwind_retains_writes_clock_and_disposes_once() {
    if !crate::test_support::child::isolated(
        "compatibility::readiness::nested::tests::actual_cdecl_nested_fatal_after_unwind_retains_writes_clock_and_disposes_once",
    ) {
        return;
    }
    let config = crate::test_support::artifacts().config();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_nested_phase_session(world).unwrap();
    let assembly = crate::test_support::artifacts().probe();
    let (manifest, object) = fixture(&host, world, &assembly);
    host.load_fixture(&manifest).unwrap();
    assert!(
        fault::NATIVE
            .set(NativeTransport::bind(&host._library).unwrap())
            .is_ok()
    );
    fault::PHASE.set(host.bridge.phase).unwrap();
    host.bridge.phase = fault::after_fixed;
    let error = host
        .advance_rational(world, 3, 100)
        .unwrap_err()
        .to_string();
    assert!(error.contains("-87"), "primary fault lost: {error}");
    assert_eq!(
        fault::state(world),
        fault::ACCEPTED.lock().unwrap().clone().unwrap()
    );
    let transform = host
        .request(world, &json!({"op":"transform.get","object":object}))
        .unwrap();
    assert_eq!(transform["position"], json!([11.0, 2.0, 1.0]));
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["stage"], "Closed");
    assert_eq!(
        status["terminal_recovery"]["native_obligations"]["remaining_routines"],
        0
    );
    let failures = host.callback_failures(0).unwrap();
    assert_eq!(failures.total, 1);
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0);
    }
    host.unload().unwrap();
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap(),
        transform
    );
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}
