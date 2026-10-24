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
    let assembly = std::path::PathBuf::from(std::env::var_os("R1_BRIDGE").unwrap());
    ManagedHostConfig {
        ffi_library: std::env::var_os("DISUNITY_IDLE_NATIVE_LIBRARY")
            .unwrap()
            .into(),
        runtime_config: assembly.with_extension("runtimeconfig.json"),
        assembly,
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
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
    host.load_fixture(&json!({"world":world,"assemblies":[std::path::PathBuf::from(std::env::var_os("DISUNITY_IDLE_NATIVE_LIBRARY").unwrap()).parent().unwrap().join("release/SharedProbe.dll")],"requirements":{"execution_profile":"native_phase_context_v1","host_mutation_profile":"native_host_idle_rename_v1"},"handles":{"owner":object,"script":script},"graph":{"nodes":[{"id":"owner","active":true,"template":false}],"components":[{"id":"script","object":"owner","script":{"type_name":"SharedProbe","fields":{}}}],"assets":[],"aliases":{}}})).unwrap();
    (host, world, object)
}
#[test]
#[ignore = "requires isolated actual CLR process"]
fn actual_host_drop_releases_failed_idle_response() {
    assert!(std::env::var_os("COMPAT_DISPLAY_CONTAINMENT").is_some());
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
    use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
    use std::io::Write;
    use std::os::fd::AsRawFd;
    static IMAGE: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();
    let image = IMAGE.get_or_init(|| {
        let fd = memfd_create(
            "sealed-native-attachment",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )
        .unwrap();
        let mut file = std::fs::File::from(fd);
        file.write_all(&std::fs::read(&config().ffi_library).unwrap())
            .unwrap();
        fcntl_add_seals(
            &file,
            SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
        )
        .unwrap();
        std::fs::File::open(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap()
    });
    // The host-selected digest names the image chosen by DISUNITY_IDLE_NATIVE_LIBRARY.
    // The lease still verifies it against the sealed bytes actually loaded.
    let selected = std::env::var("DISUNITY_IDLE_NATIVE_SHA256").unwrap_or_else(|_| {
        "dc904f14a115b3419075481d98e2e015a6a472bc8f324a38c107c9a5f033866b".into()
    });
    let lease = NativeImageLease::from_file(image.try_clone().unwrap(), &selected).unwrap();
    CompatibilityHost::boot_sealed_native(&config(), lease).unwrap()
}
