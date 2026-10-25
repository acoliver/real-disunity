use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;

use crate::{CompatibilityHost, HostError, ManagedHostConfig};

static CLAIMED: AtomicUsize = AtomicUsize::new((8 << 20) + 1);
static COPIES: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn probe(output: *mut u8, _: usize, length: *mut usize) -> i32 {
    if !output.is_null() {
        COPIES.fetch_add(1, Ordering::SeqCst);
    }
    // SAFETY: the host passes a writable length output on every probe.
    unsafe {
        *length = CLAIMED.load(Ordering::SeqCst);
    }
    -2
}
unsafe extern "C" fn observe(
    _: *const u8,
    _: usize,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
) -> i32 {
    // SAFETY: the probe receives the same output arguments supplied by the host.
    unsafe { probe(output, capacity, length) }
}
unsafe extern "C" fn dispatch(_: *const u8, _: usize) -> i32 {
    CALLS.fetch_add(1, Ordering::SeqCst);
    -1
}

fn config() -> ManagedHostConfig {
    crate::test_support::artifacts().config()
}

#[test]
fn actual_cdecl_byte_bridge_injected_bounds_and_global_reset() {
    if !crate::test_support::child::isolated(
        "compatibility::transport::byte_bridge_tests::actual_cdecl_byte_bridge_injected_bounds_and_global_reset",
    ) {
        return;
    }
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let probe = crate::test_support::artifacts().probe();
    host.load_fixture(&json!({"world":world,"assemblies":[probe],"handles":{},
        "requirements":{"execution_profile":"native_phase_context_v1"},
        "graph":{"nodes":[],"assets":[],"aliases":{},"components":[]}}))
        .unwrap();
    injected_bounds(&mut host);
    host.unload().unwrap();
    let original = host.bridge.observe;
    host.bridge.observe = observe;
    CLAIMED.store(usize::MAX, Ordering::SeqCst);
    assert!(
        matches!(host.observe(&json!({"op":"managed.statistics"})), Err(HostError::ResponseBudget(n)) if n == 8 << 20)
    );
    host.bridge.observe = original;
    host.destroy_world(world).unwrap();
    assert_eq!(COPIES.load(Ordering::SeqCst), 0);
}

fn injected_bounds(host: &mut CompatibilityHost) {
    let (original_observe, original_error, original_phase) =
        (host.bridge.observe, host.bridge.error, host.bridge.phase);
    host.bridge.observe = observe;
    host.bridge.error = probe;
    host.bridge.phase = dispatch;
    assert!(
        host.dispatch_phase(&json!({"phase":"Update","padding":"x".repeat(1 << 20)}))
            .is_err()
    );
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    assert!(
        matches!(host.dispatch_phase(&json!({"phase":"Update"})), Err(HostError::ResponseBudget(n)) if n == 8 << 20)
    );
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert!(
        matches!(host.observe(&json!({"op":"managed.statistics"})), Err(HostError::ResponseBudget(n)) if n == 8 << 20)
    );
    host.bridge.observe = original_observe;
    host.bridge.error = original_error;
    host.bridge.phase = original_phase;
}
