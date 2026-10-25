use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) static REAL: std::sync::OnceLock<super::transport::CleanupFree> =
    std::sync::OnceLock::new();
pub(super) static HANDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn fail_once(handle: u64) -> DisunityResult {
    if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
        HANDLE.store(handle, Ordering::SeqCst);
        DisunityResult {
            code: -77,
            detail: 0,
        }
    } else {
        // SAFETY: forwards only the generation-checked handle to the selected native image.
        unsafe { REAL.get().unwrap()(handle) }
    }
}
pub(super) fn config() -> ManagedHostConfig {
    crate::test_support::artifacts().config()
}
pub(super) fn loaded() -> (CompatibilityHost, u64, u64) {
    let mut host = boot();
    let world = host.create_world().unwrap();
    host.open_host_idle_phase_session(world).unwrap();
    let object = host
        .request(world, &json!({"op":"object.create","name":"before"}))
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"SharedProbe"}})).unwrap()["handle"].as_u64().unwrap();
    host.load_fixture(&json!({"world":world,"assemblies":[crate::test_support::artifacts().probe()],"requirements":{"execution_profile":"native_phase_context_v1","host_mutation_profile":"native_host_idle_rename_v1"},"handles":{"owner":object,"script":script},"graph":{"nodes":[{"id":"owner","active":true,"template":false}],"components":[{"id":"script","object":"owner","script":{"type_name":"SharedProbe","fields":{}}}],"assets":[],"aliases":{}}})).unwrap();
    (host, world, object)
}
#[test]
fn actual_host_drop_releases_failed_idle_response() {
    if !crate::test_support::child::isolated(
        "compatibility::idle_cleanup_tests::actual_host_drop_releases_failed_idle_response",
    ) {
        return;
    }
    let (mut host, world, object) = loaded();
    let allocated = host.store_statistics().unwrap().1;
    let real_free = host.native.free_for_cleanup_test();
    REAL.set(real_free).unwrap();
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    host.native.set_free_for_cleanup_test(fail_once);
    assert!(matches!(
        host.rename_object_idle(world, object, "after"),
        Err(HostIdleMutationError::OutcomeUnknown { .. })
    ));
    host.native.set_free_for_cleanup_test(real_free);
    assert_eq!(
        host.request(world, &json!({"op":"object.get","object":object}))
            .unwrap()["name"],
        "after"
    );
    assert!(
        host.request(world, &json!({"op":"session.status"}))
            .unwrap()["content_revision"]
            .as_u64()
            .unwrap()
            > before["content_revision"].as_u64().unwrap()
    );
    drop(host);
    let retry = boot();
    assert_eq!(retry.store_statistics().unwrap(), (0, allocated));
    assert_eq!(
        retry
            .native
            .probe_response_for_cleanup_test(HANDLE.load(Ordering::SeqCst)),
        1
    );
    assert!(CALLS.load(Ordering::SeqCst) >= 1);
}

pub(super) fn boot() -> CompatibilityHost {
    CompatibilityHost::boot(&config()).unwrap()
}
