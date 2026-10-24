use super::*;
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
use worldrunner::managed::{CompatibilityHost, ManagedHostConfig};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn frozen_managed() -> PathBuf {
    workspace().join(".work/resumed-managed-byte-profile/green/managed/artifacts/bin")
}

fn native() -> PathBuf {
    workspace().join(".work/resumed-native-byte-profile-complete/provider-sha256-26df6666cbdd7315c65cbc77b64ccfcaec8a337c4bcf087daa93dd2fa23ba5be.so")
}

fn config(native: PathBuf) -> ManagedHostConfig {
    let bridge = frozen_managed().join("CompatibilityBridge/debug/CompatibilityBridge.dll");
    ManagedHostConfig {
        ffi_library: native,
        runtime_config: bridge.with_extension("runtimeconfig.json"),
        assembly: bridge,
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn capture_copy(root: &Path, name: &str, source: &Path, expected: &str) -> SealedImage {
    let path = root.join(name);
    fs::copy(source, &path).unwrap();
    // Only this test-owned staging inode becomes writable or is replaced.
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&path, permissions).unwrap();
    let image = SealedImage::capture(
        root,
        Path::new(name),
        expected,
        image_lease::MAX_IMAGE_BYTES,
    )
    .unwrap();
    fs::write(path, b"invalid rewritten source image").unwrap();
    image
}

#[test]
#[ignore = "requires released frozen CLR/native tuple; execute contained qualification explicitly"]
fn sealed_native_compatibility_host_consumer() {
    let root = directory();
    let lease = capture_copy(
        root.path(),
        "native.so",
        &native(),
        "26df6666cbdd7315c65cbc77b64ccfcaec8a337c4bcf087daa93dd2fa23ba5be",
    );
    let mut host =
        CompatibilityHost::boot(&config(lease.load_path().as_path().to_path_buf())).unwrap();
    let world = host.create_world().unwrap();
    assert!(host.request(world, &json!({"op":"describe"})).is_ok());
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    std::mem::forget(host);
    std::mem::forget(lease);
}

fn own_probe() -> PathBuf {
    workspace().join(".work/resumed-image-leases/managed/bin/LeaseProbe/debug/LeaseProbe.dll")
}

fn frozen_digest(path: &Path) -> String {
    let manifest: Value = serde_json::from_slice(
        &fs::read(
            workspace().join(".work/resumed-managed-byte-profile/green/managed-artifacts.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let managed = workspace().join(".work/resumed-managed-byte-profile/green/managed");
    manifest[path.strip_prefix(managed).unwrap().to_str().unwrap()]
        .as_str()
        .unwrap()
        .into()
}

fn fixture(host: &CompatibilityHost, world: u64, probe: &SealedImage, fields: &Value) -> Value {
    let object = host
        .request(world, &json!({"op":"object.create","name":"sealed-probe"}))
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"LeaseProbe"}})).unwrap()["handle"].as_u64().unwrap();
    json!({"world":world,"assemblies":[probe.load_path().as_path()],
        "requirements":{"execution_profile":"native_phase_context_v1"},
        "handles":{"owner":object,"script":script},
        "graph":{"nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
        "components":[{"id":"script","object":"owner","script":{"type_name":"LeaseProbe","fields":fields}}]}})
}

fn observe(host: &CompatibilityHost, member: &str) -> Value {
    host.observe(&json!({"op":"managed.public","type":"LeaseProbe","id":"script","member":member}))
        .unwrap()["data"]
        .clone()
}

#[test]
#[ignore = "requires private source-bound probe and released frozen tuple; execute contained qualification explicitly"]
fn sealed_clr_pe_and_native_library_load_after_original_rewrite() {
    let root = directory();
    let probe_digest =
        fs::read_to_string(workspace().join(".work/resumed-image-leases/probe.sha256")).unwrap();
    let probe = capture_copy(root.path(), "probe.dll", &own_probe(), probe_digest.trim());
    let native_lease = capture_copy(
        root.path(),
        "native.so",
        &native(),
        "26df6666cbdd7315c65cbc77b64ccfcaec8a337c4bcf087daa93dd2fa23ba5be",
    );
    let snake = frozen_managed().join("Snake/debug/Snake.dll");
    let pe = capture_copy(root.path(), "snake.dll", &snake, &frozen_digest(&snake));
    let pdb = capture_copy(
        root.path(),
        "snake.pdb",
        &snake.with_extension("pdb"),
        &frozen_digest(&snake.with_extension("pdb")),
    );
    let mut host = CompatibilityHost::boot(&config(native())).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let fields = json!({"sealedNativePath":native_lease.load_path().as_path(),"sealedSnakePath":pe.load_path().as_path(),"sealedSnakePdbPath":pdb.load_path().as_path()});
    host.load_fixture(&fixture(&host, world, &probe, &fields))
        .unwrap();
    assert_eq!(observe(&host, "marker"), 8421);
    assert_eq!(
        observe(&host, "location"),
        probe.load_path().as_path().to_str().unwrap()
    );
    assert_eq!(observe(&host, "nativeVersion"), 2);
    assert_eq!(observe(&host, "nativeDescription"), true);
    assert_eq!(observe(&host, "nativeObjectRead"), true);
    assert_eq!(observe(&host, "nativeWorldsAfter"), 0);
    assert!(observe(&host, "snakeTypes").as_u64().unwrap() >= 8);
    host.request(world, &json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]})).unwrap();
    host.advance_rational(world, 1, 30).unwrap();
    assert_eq!(observe(&host, "updates"), 1);
    assert_eq!(observe(&host, "held"), true);
    assert!(observe(&host, "delta").as_f64().unwrap() > 0.0);
    println!(
        "TRACE {}",
        json!({"marker":observe(&host, "marker"),
        "updates":observe(&host, "updates"),"held":observe(&host, "held"),
        "delta":observe(&host, "delta"),"nativeObjectRead":observe(&host, "nativeObjectRead"),
        "nativeWorldsAfter":observe(&host, "nativeWorldsAfter"),"snakeTypes":observe(&host, "snakeTypes")})
    );
    drain(&mut host, world);
    println!(
        "PASS sealed CLR PE + portable PDB stream, sealed NativeLibrary.Load/Cdecl create/object/read/describe/destroy; host clock/input use original immutable native; live worlds/scripts/coroutines/bindings zero"
    );
    // CLR and native TLS survive host cleanup. Retain images through process exit.
    std::mem::forget((host, probe, native_lease, pe, pdb));
}

fn drain(host: &mut CompatibilityHost, world: u64) {
    host.unload().unwrap();
    let statistics = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(statistics["data"][key], 0);
    }
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}
