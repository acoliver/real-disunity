use super::idle_cleanup_tests::{HANDLE, REAL, boot, loaded};
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

static PHASE_CALLS: AtomicUsize = AtomicUsize::new(0);
static COPY: std::sync::OnceLock<super::transport::CleanupCopy> = std::sync::OnceLock::new();
static COPY_TARGET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
unsafe extern "C" fn error_after_free(handle: u64) -> DisunityResult {
    // SAFETY: the real selected native entry validates and releases this generation.
    let result = unsafe { REAL.get().unwrap()(handle) };
    if HANDLE
        .compare_exchange(0, handle, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        DisunityResult {
            code: -78,
            detail: 0,
        }
    } else {
        result
    }
}
unsafe extern "C" fn failed_copy(
    handle: u64,
    buffer: *mut u8,
    size: usize,
    required: *mut usize,
) -> DisunityResult {
    let _ = COPY_TARGET.compare_exchange(0, handle, Ordering::SeqCst, Ordering::SeqCst);
    if handle == COPY_TARGET.load(Ordering::SeqCst) {
        return DisunityResult {
            code: -66,
            detail: 0,
        };
    }
    // SAFETY: forwards the caller-owned outputs to the selected native entry.
    unsafe { COPY.get().unwrap()(handle, buffer, size, required) }
}
unsafe extern "C" fn failed_free(handle: u64) -> DisunityResult {
    if handle == COPY_TARGET.load(Ordering::SeqCst) {
        return DisunityResult {
            code: -77,
            detail: 0,
        };
    }
    // SAFETY: forwards only this generation to the pinned native image.
    unsafe { REAL.get().unwrap()(handle) }
}
unsafe extern "C" fn blocked_cleanup(_: *const u8, _: usize) -> i32 {
    PHASE_CALLS.fetch_add(1, Ordering::SeqCst);
    -88
}
#[test]
fn actual_host_release_error_after_free_reports_absent_generation() {
    if !crate::test_support::child::isolated(
        "compatibility::idle_cleanup_uncertainty_tests::actual_host_release_error_after_free_reports_absent_generation",
    ) {
        return;
    }
    let (mut host, world, object) = loaded();
    REAL.set(host.native.free_for_cleanup_test()).unwrap();
    host.native.set_free_for_cleanup_test(error_after_free);
    let error = host.rename_object_idle(world, object, "after").unwrap_err();
    assert!(error.to_string().contains("-78"));
    let error = host.retry_cleanup(world).unwrap_err().to_string();
    assert!(error.contains("generation already absent"));
    assert_eq!(
        host.native
            .probe_response_for_cleanup_test(HANDLE.load(Ordering::SeqCst)),
        1
    );
    host.destroy_world(world).unwrap();
    drop(host);
    assert_eq!(boot().store_statistics().unwrap().0, 0);
}
#[test]
fn actual_host_primary_release_cleanup_errors_keep_managed_owner() {
    if !crate::test_support::child::isolated(
        "compatibility::idle_cleanup_uncertainty_tests::actual_host_primary_release_cleanup_errors_keep_managed_owner",
    ) {
        return;
    }
    let (mut host, world, object) = loaded();
    REAL.set(host.native.free_for_cleanup_test()).unwrap();
    COPY.set(host.native.copy_for_cleanup_test()).unwrap();
    host.native.set_copy_for_cleanup_test(failed_copy);
    host.native.set_free_for_cleanup_test(failed_free);
    host.bridge.phase = blocked_cleanup;
    let error = host
        .rename_object_idle(world, object, "after")
        .unwrap_err()
        .to_string();
    for code in ["-66", "-77", "-88"] {
        assert!(error.contains(code), "{error}");
    }
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert!(roots["data"]["scripts"].as_u64().unwrap() > 0);
    drop(host);
    let report = CompatibilityHost::retained_cleanup_report().unwrap();
    assert_eq!(report["managed_loaded"], true);
    assert_eq!(report["retained"], true);
    assert!(CompatibilityHost::retry_retained_cleanup(report["id"].as_u64().unwrap()).is_err());
    assert_eq!(PHASE_CALLS.load(Ordering::SeqCst), 1);
    assert!(matches!(
        CompatibilityHost::boot(&super::idle_cleanup_tests::config()),
        Err(HostError::AlreadyBooted)
    ));
}
