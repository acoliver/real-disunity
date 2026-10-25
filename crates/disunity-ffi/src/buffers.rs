//! Explicit ownership for diagnostic bytes, independent of world lifetime.
use crate::abi::{DisunityResult, result_code};
use std::collections::BTreeMap;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

static BUFFERS: Mutex<BTreeMap<u64, Box<[u8]>>> = Mutex::new(BTreeMap::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityBuffer {
    pub handle: u64,
    pub length: u32,
}

/// Copies the last diagnostic into independently owned storage.
/// # Safety
/// `output` is writable for one buffer descriptor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_error_take(
    world: u64,
    output: *mut DisunityBuffer,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let Ok(message) = crate::store::last_error(world) else {
        return DisunityResult::invalid_handle();
    };
    let bytes = message.unwrap_or_default().into_bytes().into_boxed_slice();
    let Ok(length) = u32::try_from(bytes.len()) else {
        return DisunityResult {
            code: result_code::BUDGET,
            detail: 0,
        };
    };
    let Ok(handle) = NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
    else {
        return DisunityResult {
            code: result_code::BUDGET,
            detail: 0,
        };
    };
    BUFFERS
        .lock()
        .expect("buffer ownership panicked")
        .insert(handle, bytes);
    unsafe { *output = DisunityBuffer { handle, length } };
    DisunityResult::ok()
}

/// No partial copies. A capacity probe returns BUDGET and the required byte count.
/// # Safety
/// `output` is writable for capacity bytes, `written` for one u32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_buffer_copy(
    handle: u64,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (output.is_null() && capacity != 0) {
        return DisunityResult::invalid_argument();
    }
    let buffers = BUFFERS.lock().expect("buffer ownership panicked");
    let Some(bytes) = buffers.get(&handle) else {
        return DisunityResult::invalid_handle();
    };
    unsafe { *written = u32::try_from(bytes.len()).expect("bounded on allocation") };
    if bytes.len() > capacity as usize {
        return DisunityResult {
            code: result_code::BUDGET,
            detail: 0,
        };
    }
    if !bytes.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len()) };
    }
    DisunityResult::ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_buffer_free(handle: u64) -> DisunityResult {
    if BUFFERS
        .lock()
        .expect("buffer ownership panicked")
        .remove(&handle)
        .is_some()
    {
        DisunityResult::ok()
    } else {
        DisunityResult::invalid_handle()
    }
}
