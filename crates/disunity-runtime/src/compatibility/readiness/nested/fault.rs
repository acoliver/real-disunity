use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

use crate::compatibility::{JsonEntry, transport::NativeTransport};

pub(super) static PHASE: OnceLock<JsonEntry> = OnceLock::new();
pub(super) static NATIVE: OnceLock<NativeTransport> = OnceLock::new();
pub(super) static ACCEPTED: Mutex<Option<Value>> = Mutex::new(None);

pub(super) fn state(world: u64) -> Value {
    let read = |op| {
        NATIVE
            .get()
            .unwrap()
            .request(world, &json!({"op":op}))
            .unwrap()
    };
    json!({"time":read("time.get"),"input":read("input.status")})
}

pub(super) unsafe extern "C" fn after_fixed(bytes: *const u8, length: usize) -> i32 {
    // SAFETY: the production host supplies a live synchronous UTF-8 request.
    let request: Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(bytes, length) }).unwrap();
    // SAFETY: the retained pointer has the production Cdecl signature.
    let code = unsafe { PHASE.get().unwrap()(bytes, length) };
    if code == 0 && request["phase"] == "FixedUpdate" {
        *ACCEPTED.lock().unwrap() = Some(state(request["context"]["world"].as_u64().unwrap()));
        -87
    } else {
        code
    }
}
