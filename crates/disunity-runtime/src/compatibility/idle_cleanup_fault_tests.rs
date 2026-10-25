use super::idle_cleanup_tests::{HANDLE, REAL, boot, config, loaded};
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static RECOVER: AtomicBool = AtomicBool::new(false);
static ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn persistent(handle: u64) -> DisunityResult {
    let target = HANDLE.load(Ordering::SeqCst);
    if target == 0 {
        HANDLE.store(handle, Ordering::SeqCst);
    }
    if (target == 0 || target == handle) && !RECOVER.load(Ordering::SeqCst) {
        ATTEMPTS.fetch_add(1, Ordering::SeqCst);
        return DisunityResult {
            code: -77,
            detail: 0,
        };
    }
    // SAFETY: forwards only to the pinned native image and exact generation.
    unsafe { REAL.get().unwrap()(handle) }
}
fn failed_host() -> (CompatibilityHost, u64) {
    let (mut host, world, object) = loaded();
    REAL.set(host.native.free_for_cleanup_test()).unwrap();
    host.native.set_free_for_cleanup_test(persistent);
    let error = host.rename_object_idle(world, object, "after").unwrap_err();
    assert!(error.to_string().contains("-77"));
    assert!(
        host.request(
            world,
            &json!({"op":"object.set","object":object,"name":"wrong"})
        )
        .is_err()
    );
    assert!(host.open_host_idle_phase_session(world).is_err());
    assert!(host.load_fixture(&json!({"world":world})).is_err());
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "bindings", "coroutines"] {
        assert_eq!(roots["data"][key], 0);
    }
    assert_eq!(roots["data"]["loaded"], false);
    (host, world)
}
#[test]
fn actual_host_persistent_release_keeps_owner_and_recovers() {
    if !crate::test_support::child::isolated(
        "compatibility::idle_cleanup_fault_tests::actual_host_persistent_release_keeps_owner_and_recovers",
    ) {
        return;
    }
    let (mut host, world) = failed_host();
    assert!(
        host.retry_cleanup(world)
            .unwrap_err()
            .to_string()
            .contains("-77")
    );
    assert_eq!(ATTEMPTS.load(Ordering::SeqCst), 2);
    drop(host);
    let report = CompatibilityHost::retained_cleanup_report().unwrap();
    assert_eq!(report["retained"], true);
    assert_eq!(report["responses"], 1);
    assert_eq!(report["worlds"], json!([world]));
    assert_eq!(ATTEMPTS.load(Ordering::SeqCst), 3);
    assert!(matches!(
        CompatibilityHost::boot(&config()),
        Err(HostError::AlreadyBooted)
    ));
    recover_retained(report);
}
fn recover_retained(report: serde_json::Value) {
    let id = report["id"].as_u64().unwrap();
    assert!(
        std::thread::spawn(move || CompatibilityHost::retry_retained_cleanup(id))
            .join()
            .unwrap()
            .is_err()
    );
    assert!(CompatibilityHost::retry_retained_cleanup(id).is_err());
    assert_eq!(ATTEMPTS.load(Ordering::SeqCst), 4);
    RECOVER.store(true, Ordering::SeqCst);
    let resolved = CompatibilityHost::retry_retained_cleanup(id).unwrap();
    assert_eq!(resolved["retained"], false);
    let retry = boot();
    assert_eq!(retry.store_statistics().unwrap().0, 0);
    assert_eq!(
        retry
            .native
            .probe_response_for_cleanup_test(HANDLE.load(Ordering::SeqCst)),
        1
    );
}
#[test]
fn actual_host_thread_exit_keeps_claim_and_reportable_owner() {
    if !crate::test_support::child::isolated(
        "compatibility::idle_cleanup_fault_tests::actual_host_thread_exit_keeps_claim_and_reportable_owner",
    ) {
        return;
    }
    std::thread::spawn(|| {
        let (host, _) = failed_host();
        drop(host);
    })
    .join()
    .unwrap();
    let report = CompatibilityHost::retained_cleanup_report().unwrap();
    assert_eq!(report["retained"], true);
    assert_eq!(report["owner_thread_alive"], false);
    assert_eq!(report["responses"], 1);
    assert!(matches!(
        CompatibilityHost::boot(&config()),
        Err(HostError::AlreadyBooted)
    ));
    assert!(CompatibilityHost::retry_retained_cleanup(report["id"].as_u64().unwrap()).is_err());
}

#[test]
fn actual_host_cleanup_rejects_foreign_world_without_releasing_response() {
    if !crate::test_support::child::isolated(
        "compatibility::idle_cleanup_fault_tests::actual_host_cleanup_rejects_foreign_world_without_releasing_response",
    ) {
        return;
    }
    let (mut host, world) = failed_host();
    let foreign = host.native.create_world().unwrap();
    assert!(matches!(
        host.retry_cleanup(foreign),
        Err(HostError::NoWorld)
    ));
    assert!(matches!(
        host.destroy_world(foreign),
        Err(HostError::NoWorld)
    ));
    assert_eq!(ATTEMPTS.load(Ordering::SeqCst), 1);
    assert_eq!(host.native.idle_response_count(), 1);
    host.native.destroy_world(foreign).unwrap();
    assert!(host.worlds.contains(&world));
    drop(host);
    let report = CompatibilityHost::retained_cleanup_report().unwrap();
    let id = report["id"].as_u64().unwrap();
    assert!(CompatibilityHost::retry_retained_cleanup(id + 1).is_err());
    assert_eq!(ATTEMPTS.load(Ordering::SeqCst), 2);
    RECOVER.store(true, Ordering::SeqCst);
    assert_eq!(
        CompatibilityHost::retry_retained_cleanup(id).unwrap()["retained"],
        false
    );
    assert_eq!(boot().store_statistics().unwrap().0, 0);
}
