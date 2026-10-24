use super::*;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use worldrunner_disunity_runtime::{CompatibilityHost, HostError, ManagedHostConfig};

const NATIVE_SHA: &str = "26df6666cbdd7315c65cbc77b64ccfcaec8a337c4bcf087daa93dd2fa23ba5be";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn work() -> PathBuf {
    root().join(".work/resumed-sealed-native-host")
}
fn config() -> ManagedHostConfig {
    let bridge = root().join(".work/resumed-managed-byte-profile/green/managed/artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll");
    ManagedHostConfig {
        ffi_library: "/ignored/nonexistent/native.so".into(),
        runtime_config: bridge.with_extension("runtimeconfig.json"),
        assembly: bridge,
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn captured_native() -> File {
    let original = root().join(format!(
        ".work/resumed-native-byte-profile-complete/provider-sha256-{NATIVE_SHA}.so"
    ));
    let staging = work().join(format!("native-staging-{}.so", std::process::id()));
    fs::copy(original, &staging).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o600)).unwrap();
    let bytes = fs::read(&staging).unwrap();
    assert_eq!(digest(&bytes), NATIVE_SHA);
    let file = sealed(&bytes);
    fs::write(&staging, b"owned staging source corrupted BEFORE boot").unwrap();
    assert_ne!(digest(&fs::read(&staging).unwrap()), NATIVE_SHA);
    fs::remove_file(staging).unwrap();
    file
}

fn observe(host: &CompatibilityHost, member: &str) -> Value {
    host.observe(&json!({"op":"managed.public","type":"SharedProbe","id":"script","member":member}))
        .unwrap()["data"]
        .clone()
}

fn manifest(host: &CompatibilityHost, world: u64) -> (Value, u64) {
    let owner = host
        .request(
            world,
            &json!({"op":"object.create","name":"rust-created-object-9163"}),
        )
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":owner,"kind":"Script","fields":{"type":"SharedProbe"}})).unwrap()["handle"].as_u64().unwrap();
    (
        json!({"world":world,"assemblies":[work().join("release/SharedProbe.dll")],
        "requirements":{"execution_profile":"native_phase_context_v1"},
        "handles":{"owner":owner,"script":script},
        "graph":{"nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
        "components":[{"id":"script","object":"owner","script":{"type_name":"SharedProbe","fields":{}}}]}}),
        owner,
    )
}

#[test]
#[ignore = "actual CLR qualification requires private display/network/mount containment"]
fn same_sealed_native_world_bidirectional_after_staging_corruption() {
    assert!(std::env::var_os("COMPAT_DISPLAY_CONTAINMENT").is_some());
    let file = captured_native();
    let inode = file.metadata().unwrap().ino();
    let peer = file.try_clone().unwrap();
    let lease = NativeImageLease::from_file(file, NATIVE_SHA).unwrap();
    let mut host = CompatibilityHost::boot_sealed_native(&config(), lease).unwrap();
    let reentrant = CompatibilityHost::boot_sealed_native(
        &config(),
        NativeImageLease::from_file(peer.try_clone().unwrap(), NATIVE_SHA).unwrap(),
    );
    assert!(matches!(reentrant, Err(HostError::AlreadyBooted)));
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (fixture, object) = manifest(&host, world);
    host.load_fixture(&fixture).unwrap();
    let reverse = assert_bidirectional(&host, world, object);
    exercise_frame(&mut host, world, object);
    record_mapping(inode, world, object, reverse);
    drain(&mut host, world);
    drop(host);
    retry_after_drop(peer, world);
    println!(
        "PASS SAME sealed native store; Rust/CLR bidirectional mutation, Update, retry, retained image and cleanup"
    );
}

fn assert_bidirectional(host: &CompatibilityHost, world: u64, object: u64) -> u64 {
    assert_eq!(observe(host, "awakeCount"), 1);
    assert_eq!(observe(host, "initialName"), "rust-created-object-9163");
    assert_eq!(
        host.request(world, &json!({"op":"object.get","object":object}))
            .unwrap()["name"],
        "clr-mutated-rust-object-7291"
    );
    let reverse = observe(host, "reverse").as_u64().unwrap();
    assert_ne!(reverse, 0);
    assert!(reverse >> 32 > 0);
    assert_eq!(
        host.request(world, &json!({"op":"object.get","object":reverse}))
            .unwrap()["name"],
        "clr-created-object-4837"
    );
    reverse
}

fn record_mapping(inode: u64, world: u64, object: u64, reverse: u64) {
    let maps = fs::read_to_string("/proc/self/maps").unwrap();
    let mappings: Vec<_> = maps
        .lines()
        .filter(|line| line.contains("memfd:sealed-native-attachment"))
        .collect();
    assert!(!mappings.is_empty());
    assert!(
        mappings
            .iter()
            .all(|line| line.split_whitespace().nth(4).unwrap() == inode.to_string())
    );
    println!(
        "SAME_IMAGE {}",
        json!({"inode":inode,"sha256":NATIVE_SHA,"world":world,"rustObject":object,"clrObject":reverse,"maps":mappings})
    );
}

fn retry_after_drop(peer: File, world: u64) {
    assert!(peer.metadata().unwrap().len() > 0);
    let foreign = CompatibilityHost::boot_sealed_native(
        &config(),
        NativeImageLease::from_file(captured_native(), NATIVE_SHA).unwrap(),
    );
    assert!(
        foreign
            .err()
            .unwrap()
            .to_string()
            .contains("different process image")
    );
    let mut retry = CompatibilityHost::boot_sealed_native(
        &config(),
        NativeImageLease::from_file(peer, NATIVE_SHA).unwrap(),
    )
    .unwrap();
    let second = retry.create_world().unwrap();
    assert_ne!(second, world);
    retry.open_phase_session(second).unwrap();
    let (fixture, _) = manifest(&retry, second);
    retry.load_fixture(&fixture).unwrap();
    assert_eq!(observe(&retry, "awakeCount"), 1);
    drain(&mut retry, second);
    drop(retry);
    assert!(
        fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("memfd:sealed-native-attachment")
    );
}

fn exercise_frame(host: &mut CompatibilityHost, world: u64, object: u64) {
    host.request(
        world,
        &json!({"op":"object.set","object":object,"name":"rust-mutated-again-1529"}),
    )
    .unwrap();
    host.request(world, &json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]})).unwrap();
    host.advance_rational(world, 1, 30).unwrap();
    assert_eq!(observe(host, "updates"), 1);
    assert_eq!(observe(host, "rustName"), "rust-mutated-again-1529");
    assert_eq!(observe(host, "held"), true);
    assert_eq!(observe(host, "nativeHeld"), true);
    assert_eq!(observe(host, "delta"), observe(host, "nativeDelta"));
    assert!((observe(host, "delta").as_f64().unwrap() - 1.0 / 30.0).abs() < 1e-6);
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap()["position"],
        json!([17., 23., 31.])
    );
    host.advance_rational(world, 1, 30).unwrap();
    assert_eq!(observe(host, "updates"), 2);
    assert_eq!(observe(host, "resumed"), 1);
    assert_eq!(host.callback_failures(0).unwrap().total, 0);
    println!(
        "CALLBACK {}",
        json!({"awake":observe(host,"awakeCount"),"updates":observe(host,"updates"),"resumed":observe(host,"resumed"),"W":observe(host,"held"),"delta":observe(host,"delta")})
    );
}

fn drain(host: &mut CompatibilityHost, world: u64) {
    host.unload().unwrap();
    let stats = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(stats["data"][key], 0);
    }
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    println!("CLEANUP {}", stats["data"]);
}

#[test]
#[ignore = "intentional legacy RED: canonical path boot does not support sealed procfd"]
fn legacy_canonical_route_sealed_procfd_red() {
    assert!(std::env::var_os("COMPAT_DISPLAY_CONTAINMENT").is_some());
    let file = captured_native();
    let mut legacy = config();
    legacy.ffi_library = format!("/proc/self/fd/{}", file.as_raw_fd()).into();
    // SAFETY: this exact trusted native image is pinned for this isolated RED process.
    let library = unsafe { libloading::Library::new(&legacy.ffi_library).unwrap() };
    std::mem::forget(library);
    let _host = CompatibilityHost::boot(&legacy).unwrap();
    std::mem::forget(file);
}
