use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

use super::super::{CompatibilityHost, JsonEntry, transport::NativeTransport};

pub(super) static PHASE: OnceLock<JsonEntry> = OnceLock::new();
pub(super) static LOAD: OnceLock<JsonEntry> = OnceLock::new();
pub(super) static CONTACT: OnceLock<JsonEntry> = OnceLock::new();
static NATIVE: OnceLock<NativeTransport> = OnceLock::new();
pub(super) static ACCEPTED: Mutex<Option<Value>> = Mutex::new(None);

pub(super) fn install(host: &CompatibilityHost) {
    assert!(
        NATIVE
            .set(NativeTransport::bind(&host._library).unwrap())
            .is_ok()
    );
}

pub(super) fn state(host: &CompatibilityHost, world: u64) -> Value {
    snapshot(&host.native, world)
}

fn snapshot(native: &NativeTransport, world: u64) -> Value {
    let read = |op: &str| native.request(world, &json!({"op":op})).unwrap();
    json!({"world":world,"time":read("time.get"),"input_status":read("input.status"),
        "dynamic":native.request(world, &json!({"op":"input.get","phase":"dynamic"})).unwrap(),
        "fixed":native.request(world, &json!({"op":"input.get","phase":"fixed"})).unwrap()})
}

fn capture(request: &Value) {
    let world = request["context"]["world"]
        .as_u64()
        .or_else(|| request["world"].as_u64())
        .unwrap();
    *ACCEPTED.lock().unwrap() = Some(snapshot(NATIVE.get().unwrap(), world));
}

// Injected transport failure after real Cdecl execution and scope unwind.
pub(super) unsafe extern "C" fn after_fixed(bytes: *const u8, length: usize) -> i32 {
    // SAFETY: the host supplies a live synchronous UTF-8 buffer.
    let request: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(bytes, length) }).unwrap();
    // SAFETY: this is the correctly typed production Cdecl entry.
    let code = unsafe { PHASE.get().unwrap()(bytes, length) };
    if code == 0 && request["phase"] == "FixedUpdate" {
        capture(&request);
        -77
    } else {
        code
    }
}

pub(super) unsafe extern "C" fn after_load(bytes: *const u8, length: usize) -> i32 {
    // SAFETY: the host supplies a live synchronous UTF-8 buffer.
    let request: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(bytes, length) }).unwrap();
    // SAFETY: this is the correctly typed production Cdecl entry.
    let code = unsafe { LOAD.get().unwrap()(bytes, length) };
    if code == 0 {
        capture(&request);
        -78
    } else {
        code
    }
}

// The real Contacts claim reads and rejects this deliberately malformed envelope.
pub(super) unsafe extern "C" fn malformed_contacts(bytes: *const u8, length: usize) -> i32 {
    // SAFETY: the host supplies a live synchronous UTF-8 buffer.
    let mut request: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(bytes, length) }).unwrap();
    capture(&request);
    request["events_response"] = json!("{\"version\":1,\"ok\":true,\"data\":[]}");
    let malformed = serde_json::to_vec(&request).unwrap();
    // SAFETY: the actual entry reads this owned buffer synchronously.
    unsafe { CONTACT.get().unwrap()(malformed.as_ptr(), malformed.len()) }
}
