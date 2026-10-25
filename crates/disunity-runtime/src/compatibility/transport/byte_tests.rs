use super::*;
use serde_json::json;

fn native() -> (std::mem::ManuallyDrop<Library>, NativeTransport) {
    let config = crate::test_support::artifacts().config();
    let library = crate::load_ffi_image(&config).unwrap();
    let transport = NativeTransport::bind(&library).unwrap();
    (std::mem::ManuallyDrop::new(library), transport)
}

#[test]
fn actual_native_byte_requests_are_bounded_and_responses_release_synchronously() {
    if !crate::test_support::child::isolated(
        "compatibility::transport::byte_tests::actual_native_byte_requests_are_bounded_and_responses_release_synchronously",
    ) {
        return;
    }
    let (_library, transport) = native();
    let world = transport.create_world().unwrap();
    let mut create = json!({"version":1,"op":"object.create","name":""});
    let overhead = serde_json::to_vec(&create).unwrap().len();
    create["name"] = json!("x".repeat((1 << 20) - overhead));
    transport.request(world, &create).unwrap();
    create["name"] = json!("x".repeat((1 << 20) + 1 - overhead));
    assert!(transport.request(world, &create).is_err());
    let snapshot = transport.request(world, &json!({"op":"snapshot"})).unwrap();
    assert_eq!(snapshot["objects"].as_array().unwrap().len(), 1);
    for _ in 0..1100 {
        transport.request(world, &json!({"op":"time.get"})).unwrap();
    }
    transport.destroy_world(world).unwrap();
    assert_eq!(transport.statistics().unwrap().0, 0);
}

fn sized_transaction(transport: &NativeTransport, world: u64) -> Value {
    let large = transport
        .request(
            world,
            &json!({"op":"object.create","name":"x".repeat(600_000)}),
        )
        .unwrap()["handle"]
        .clone();
    let tail = transport
        .request(world, &json!({"op":"object.create"}))
        .unwrap()["handle"]
        .clone();
    let mut operations = vec![json!({"op":"object.get","object":large}); 13];
    operations.push(json!({"op":"object.set","object":tail,"name":""}));
    let mut request = json!({"version":1,"op":"transaction","operations":operations});
    let base = transport.request_bytes(world, &request).unwrap().len();
    request["operations"][13]["name"] = json!("p".repeat((8 << 20) - base));
    request
}

#[test]
fn actual_native_byte_exact_eight_mib_copy_and_native_oversize_rollback() {
    if !crate::test_support::child::isolated(
        "compatibility::transport::byte_tests::actual_native_byte_exact_eight_mib_copy_and_native_oversize_rollback",
    ) {
        return;
    }
    let (_library, transport) = native();
    let world = transport.create_world().unwrap();
    let mut request = sized_transaction(&transport, world);
    for _ in 0..3 {
        assert_eq!(
            transport.request_bytes(world, &request).unwrap().len(),
            8 << 20
        );
    }
    let before = transport.request(world, &json!({"op":"snapshot"})).unwrap();
    let tail = request["operations"][13]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    request["operations"][13]["name"] = json!(tail + "x");
    assert!(transport.request(world, &request).is_err());
    assert_eq!(
        transport.request(world, &json!({"op":"snapshot"})).unwrap(),
        before
    );
    transport.destroy_world(world).unwrap();
}

#[test]
fn injected_cdecl_byte_copy_probe_rejects_before_allocation_and_keeps_cleanup_error() {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static FREE: OnceLock<FreeResponse> = OnceLock::new();
    static RELEASES: AtomicUsize = AtomicUsize::new(0);
    static COPIES: AtomicUsize = AtomicUsize::new(0);
    static MODE: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn copy(
        _: u64,
        output: *mut u8,
        _: usize,
        length: *mut usize,
    ) -> DisunityResult {
        if !output.is_null() {
            COPIES.fetch_add(1, Ordering::SeqCst);
        }
        // SAFETY: the transport supplies this writable output for each probe.
        unsafe {
            *length = usize::MAX;
        }
        DisunityResult { code: 2, detail: 0 }
    }
    unsafe extern "C" fn free(handle: u64) -> DisunityResult {
        RELEASES.fetch_add(1, Ordering::SeqCst);
        // SAFETY: this is the retained handle issued by the real selected native image.
        let result = unsafe { FREE.get().unwrap()(handle) };
        assert_eq!(result.code, 0);
        DisunityResult {
            code: MODE.load(Ordering::SeqCst) as i32,
            detail: 0,
        }
    }
    let (_library, mut transport) = native();
    let world = transport.create_world().unwrap();
    FREE.set(transport.free).unwrap();
    transport.copy = copy;
    transport.free = free;
    for mode in [0, 3] {
        MODE.store(mode, Ordering::SeqCst);
        RELEASES.store(0, Ordering::SeqCst);
        let error = transport
            .request(world, &json!({"op":"time.get"}))
            .unwrap_err()
            .to_string();
        assert!(error.contains(&(8 << 20).to_string()), "{error}");
        if mode == 3 {
            assert!(error.contains("response release"), "{error}");
        }
        assert_eq!(RELEASES.load(Ordering::SeqCst), 1);
        assert_eq!(COPIES.load(Ordering::SeqCst), 0);
    }
    transport.destroy_world(world).unwrap();
}
