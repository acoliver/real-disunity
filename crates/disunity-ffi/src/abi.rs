//! The exported C ABI. Blittable types only, Cdecl, every call returns
//! [`DisunityResult`]. This file is a csbindgen input: adding an export
//! requires explicitly updating `csharp/Disunity/NativeBindings.g.cs` (see
//! `csharp/README.md`) and a matching `disunity_interface.json` entry.
//!
//! The interface is append-only: never change an existing export's signature;
//! add a new export and bump the manifest.

pub use crate::DisunityCommand;

use crate::{MAX_COMMANDS_PER_SUBMIT, command_kind};

/// `code` values for [`DisunityResult`].
pub mod result_code {
    pub const OK: i32 = 0;
    pub const INVALID_HANDLE: i32 = 1;
    pub const BUDGET: i32 = 2;
    pub const ERROR: i32 = 3;
    pub const INVALID_ARGUMENT: i32 = 4;
}

/// Result of every ABI call. `detail` carries `0` unless `code` is `ERROR`,
/// where it is a fetch handle for `disunity_error_message`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisunityResult {
    pub code: i32,
    pub detail: u64,
}

impl DisunityResult {
    const fn ok() -> Self {
        Self {
            code: result_code::OK,
            detail: 0,
        }
    }

    fn invalid_handle() -> Self {
        Self {
            code: result_code::INVALID_HANDLE,
            detail: 0,
        }
    }

    fn budget() -> Self {
        Self {
            code: result_code::BUDGET,
            detail: 0,
        }
    }

    fn error(detail: u64) -> Self {
        Self {
            code: result_code::ERROR,
            detail,
        }
    }

    fn invalid_argument() -> Self {
        Self {
            code: result_code::INVALID_ARGUMENT,
            detail: 0,
        }
    }
}

/// The ABI version of this library; see `disunity_interface.json`.
pub const DISUNITY_ABI_VERSION: u32 = 2;

fn translate_error(world: Option<u64>, reason: &str) -> DisunityResult {
    let detail = 1;
    let mut store = crate::lock_store();
    crate::record_error(&mut store, world, reason);
    DisunityResult::error(detail)
}

fn mutation_error(reason: &str) -> DisunityResult {
    if reason == crate::integrity::WRONG_THREAD {
        DisunityResult::invalid_argument()
    } else {
        DisunityResult::invalid_handle()
    }
}

/// Writes the ABI version this library implements into `out_version`.
///
/// # Safety
/// `out_version` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_abi_version(out_version: *mut u32) -> DisunityResult {
    if out_version.is_null() {
        return DisunityResult::invalid_argument();
    }
    unsafe { *out_version = DISUNITY_ABI_VERSION };
    DisunityResult::ok()
}

/// Creates a world and writes its handle into `out_world`.
///
/// # Safety
/// `out_world` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_create(out_world: *mut u64) -> DisunityResult {
    if out_world.is_null() {
        return DisunityResult::invalid_argument();
    }
    let mut store = crate::lock_store();
    unsafe { *out_world = crate::create_world(&mut store) };
    DisunityResult::ok()
}

/// Destroys a world and every object in it.
///
/// # Safety
/// `world` must be a live handle from `disunity_world_create` and must not be
/// used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_destroy(world: u64) -> DisunityResult {
    let mut store = crate::lock_store();
    match crate::destroy_world(&mut store, world) {
        Ok(()) => DisunityResult::ok(),
        Err(reason) => mutation_error(reason),
    }
}

/// Creates an object in `world` and writes its handle into `out_object`.
///
/// # Safety
/// `out_object` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_gameobject_create(
    world: u64,
    out_object: *mut u64,
) -> DisunityResult {
    if out_object.is_null() {
        return DisunityResult::invalid_argument();
    }
    let mut store = crate::lock_store();
    match crate::create_object(&mut store, world) {
        Ok(handle) => {
            unsafe { *out_object = handle };
            DisunityResult::ok()
        }
        Err(reason) => mutation_error(reason),
    }
}

/// Destroys an object; outstanding handles to it become invalid.
///
/// # Safety
/// `world` must be live and `object` must belong to it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_gameobject_destroy(world: u64, object: u64) -> DisunityResult {
    let mut store = crate::lock_store();
    match crate::destroy_object(&mut store, world, object) {
        Ok(()) => DisunityResult::ok(),
        Err(reason) => mutation_error(reason),
    }
}

/// Construction-time transform placement. Per-frame movement goes through the
/// command buffer instead; one FFI call per object per frame is the pattern
/// this ABI exists to prevent.
/// See `disunity_interface.json`.
///
/// # Safety
/// `world` must be live; pointers must be valid for writes as stated in
/// `disunity_interface.json`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_set_position(
    world: u64,
    object: u64,
    x: f32,
    y: f32,
    z: f32,
) -> DisunityResult {
    unsafe {
        disunity_command_buffer_submit(
            world,
            &DisunityCommand {
                object,
                kind: command_kind::SET_POSITION,
                x,
                y,
                z,
            },
            1,
        )
    }
}

/// Construction-time yaw placement in radians.
/// See `disunity_interface.json`.
///
/// # Safety
/// `world` must be live; pointers must be valid for writes as stated in
/// `disunity_interface.json`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_set_rotation_y(
    world: u64,
    object: u64,
    radians: f32,
) -> DisunityResult {
    unsafe {
        disunity_command_buffer_submit(
            world,
            &DisunityCommand {
                object,
                kind: command_kind::SET_ROTATION_Y,
                x: radians,
                y: 0.0,
                z: 0.0,
            },
            1,
        )
    }
}

/// Copies a command batch into engine storage. The buffer is consumed before
/// the call returns; nothing retains the pointer.
///
/// # Safety
/// `commands` must be valid for reads of `count` elements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_command_buffer_submit(
    world: u64,
    commands: *const DisunityCommand,
    count: u32,
) -> DisunityResult {
    if count > MAX_COMMANDS_PER_SUBMIT || usize::try_from(count).is_err() {
        // A batch over budget reports BUDGET even when the pointer is also
        // unusable; the size verdict is the caller's actionable answer.
        return DisunityResult::budget();
    }
    if commands.is_null() && count > 0 {
        return DisunityResult::invalid_argument();
    }
    let slice = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(commands, count as usize) }
    };
    let mut store = crate::lock_store();
    match crate::submit_commands(&mut store, world, slice) {
        Ok(()) => DisunityResult::ok(),
        Err(reason) => mutation_error(reason),
    }
}
/// Advances the world one tick, draining any submitted command buffer.
///
/// # Safety
/// `world` must be live; `dt` and `frame` must be finite.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_step(world: u64, dt: f32, frame: u64) -> DisunityResult {
    // The guard must release before translate_error re-locks the store to
    // record the message; holding both is a self-deadlock on the error path.
    let outcome = {
        let mut store = crate::lock_store();
        crate::step_world(&mut store, world, dt, frame)
    };
    match outcome {
        Ok(()) => DisunityResult::ok(),
        Err(reason) if reason == crate::integrity::WRONG_THREAD => {
            DisunityResult::invalid_argument()
        }
        Err(reason) => translate_error(Some(world), &reason),
    }
}

/// Copies the world's last error message into `buffer` (no terminator) and
/// writes the byte length into `out_len`. A `BUDGET` result means the buffer
/// was too small and `out_len` carries the required size.
///
/// # Safety
/// `buffer` must be valid for writes of `capacity` bytes; `out_len` for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_error_message(
    world: u64,
    buffer: *mut u8,
    capacity: u32,
    out_len: *mut u32,
) -> DisunityResult {
    if buffer.is_null() || out_len.is_null() {
        return DisunityResult::invalid_argument();
    }
    let message = crate::with_world(world, |world| world.last_error.clone());
    match message {
        Err(_) => DisunityResult::invalid_handle(),
        Ok(None) => {
            unsafe { *out_len = 0 };
            DisunityResult::ok()
        }
        Ok(Some(message)) => {
            let bytes = message.as_bytes();
            let capacity = capacity as usize;
            if bytes.len() > capacity {
                unsafe { *out_len = u32::try_from(bytes.len()).unwrap_or(u32::MAX) };
                return DisunityResult::budget();
            }
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
                *out_len = bytes.len() as u32;
            }
            DisunityResult::ok()
        }
    }
}

/// Writes live and allocated slot counts, the soak test's drain check.
///
/// # Safety
/// Both out pointers must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_store_statistics(
    out_live: *mut u64,
    out_allocated: *mut u64,
) -> DisunityResult {
    if out_live.is_null() || out_allocated.is_null() {
        return DisunityResult::invalid_argument();
    }
    let (live, allocated) = crate::store_statistics();
    unsafe {
        *out_live = live as u64;
        *out_allocated = allocated as u64;
    }
    DisunityResult::ok()
}

/// Bulk object-state extraction for host renderers: one call per frame, not
/// one per object. `written` may exceed the capacity to signal truncation.
/// See `disunity_interface.json`.
///
/// # Safety
/// `world` must be live; pointers must be valid for writes as stated in
/// `disunity_interface.json`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_export_objects(
    world: u64,
    states: *mut crate::DisunityObjectState,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() {
        return DisunityResult::invalid_argument();
    }
    let buffer = if states.is_null() || capacity == 0 {
        &mut []
    } else {
        unsafe { std::slice::from_raw_parts_mut(states, usize::try_from(capacity).unwrap_or(0)) }
    };
    unsafe {
        crate::export_object_states(world, buffer, &mut *written).map_or_else(
            |reason| translate_error(Some(world), reason),
            |()| DisunityResult::ok(),
        )
    }
}

/// Executes one UTF-8 JSON request and returns an independently owned response.
///
/// # Safety
/// `request` must address `len` readable bytes and `response` one writable u64.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_request(
    world: u64,
    request: *const u8,
    len: usize,
    response: *mut u64,
) -> DisunityResult {
    if response.is_null() {
        return DisunityResult::invalid_argument();
    }
    unsafe {
        *response = 0;
    }
    if len > crate::response::MAX_REQUEST_BYTES {
        return DisunityResult::budget();
    }
    if request.is_null() || len == 0 {
        return DisunityResult::invalid_argument();
    }
    let bytes = unsafe { std::slice::from_raw_parts(request, len) };
    match crate::response::request_bytes(world, bytes) {
        Ok(handle) => {
            unsafe {
                *response = handle;
            }
            DisunityResult::ok()
        }
        Err(code) => DisunityResult { code, detail: 0 },
    }
}

/// Copies an owned response; zero-capacity null buffer is a length probe.
///
/// # Safety
/// `len` must be writable. A nonnull `buffer` must address `cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_response_copy(
    response: u64,
    buffer: *mut u8,
    cap: usize,
    len: *mut usize,
) -> DisunityResult {
    if len.is_null() || (buffer.is_null() && cap != 0) {
        return DisunityResult::invalid_argument();
    }
    unsafe {
        *len = 0;
    }
    match crate::response::response_bytes(response) {
        Err(code) => DisunityResult { code, detail: 0 },
        Ok(bytes) => {
            unsafe {
                *len = bytes.len();
            }
            if cap < bytes.len() {
                return DisunityResult::budget();
            }
            if !bytes.is_empty() {
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
                }
            }
            DisunityResult::ok()
        }
    }
}

/// Frees an owned response, invalidating its generation-checked handle.
///
/// # Safety
/// No caller pointers are retained or dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_response_free(response: u64) -> DisunityResult {
    match crate::response::response_free(response) {
        Ok(()) => DisunityResult::ok(),
        Err(code) => DisunityResult { code, detail: 0 },
    }
}
