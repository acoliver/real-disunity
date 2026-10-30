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

/// Stable categories carried in DisunityResult.detail for native errors.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisunityErrorReason {
    InvalidArgument = 1,
    InvalidTime = 2,
    WrongCallback = 3,
    CallbackFailed = 4,
    Faulted = 5,
    AbortedScope = 6,
    Capacity = 7,
    SingularParent = 8,
    Physics = 9,
    StaleHandle = 10,
    AdvanceInProgress = 11,
    InvalidComponent = 12,
    Ui = 13,
    NotReady = 14,
}

/// Result of every ABI call. For ERROR, detail is a DisunityErrorReason tag.
/// Diagnostic text is copied through disunity_error_take and explicitly freed.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisunityResult {
    pub code: i32,
    pub detail: u64,
}

impl DisunityResult {
    pub(crate) const fn ok() -> Self {
        Self {
            code: result_code::OK,
            detail: 0,
        }
    }

    pub(crate) fn invalid_handle() -> Self {
        Self {
            code: result_code::INVALID_HANDLE,
            detail: DisunityErrorReason::StaleHandle as u64,
        }
    }

    fn budget() -> Self {
        Self {
            code: result_code::BUDGET,
            detail: DisunityErrorReason::Capacity as u64,
        }
    }

    fn error(detail: u64) -> Self {
        Self {
            code: result_code::ERROR,
            detail,
        }
    }

    pub(crate) fn invalid_argument() -> Self {
        Self {
            code: result_code::INVALID_ARGUMENT,
            detail: DisunityErrorReason::InvalidArgument as u64,
        }
    }
}
pub(crate) fn allocation_failure(error: crate::store::AllocationError) -> DisunityResult {
    use crate::store::AllocationError;
    use worldrunner_world_simulation::WorldError;
    let reason = match &error {
        AllocationError::InvalidWorld => DisunityErrorReason::StaleHandle,
        AllocationError::HandleCapacity => DisunityErrorReason::Capacity,
        AllocationError::Native(error) => crate::errors::AbiError::Native(error.clone()).reason(),
    };
    let mut result = match error {
        AllocationError::HandleCapacity
        | AllocationError::Native(WorldError::CapacityExceeded(_)) => DisunityResult::budget(),
        AllocationError::InvalidWorld
        | AllocationError::Native(
            WorldError::WrongWorld | WorldError::StaleObject(_) | WorldError::StaleComponent(_),
        ) => DisunityResult::invalid_handle(),
        AllocationError::Native(
            WorldError::InvalidComponent(_)
            | WorldError::InvalidObject
            | WorldError::HierarchyCycle
            | WorldError::SingularParent
            | WorldError::NonRepresentableLocalTransform
            | WorldError::InvalidFields
            | WorldError::NotScript
            | WorldError::WrongMutation
            | WorldError::Ui(_)
            | WorldError::Physics(_)
            | WorldError::InvalidTime(_)
            | WorldError::InputOrder
            | WorldError::InputCapacity
            | WorldError::InvalidInput
            | WorldError::AdvanceInProgress
            | WorldError::WrongCallback
            | WorldError::InvalidContact
            | WorldError::NotReady
            | WorldError::CallbackFailed(_)
            | WorldError::Faulted,
        ) => DisunityResult::invalid_argument(),
    };
    result.detail = reason as u64;
    result
}

/// The ABI version of this library; see `disunity_interface.json`.
pub const DISUNITY_ABI_VERSION: u32 = 7;

pub(crate) fn translate_error(
    world: Option<u64>,
    reason: &crate::errors::AbiError,
) -> DisunityResult {
    let mut store = crate::lock_store();
    crate::record_error(&mut store, world, &reason.to_string());
    DisunityResult::error(reason.reason() as u64)
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
    match crate::create_world(&mut store) {
        Ok(handle) => {
            unsafe { *out_world = handle };
            DisunityResult::ok()
        }
        Err(error) => allocation_failure(error),
    }
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
        Err(_) => DisunityResult::invalid_handle(),
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
        Err(error) => allocation_failure(error),
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
        Err(_) => DisunityResult::invalid_handle(),
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
    let result = {
        let mut store = crate::lock_store();
        crate::commands::apply_immediate(
            &mut store,
            world,
            DisunityCommand {
                object,
                kind: command_kind::SET_POSITION,
                x,
                y,
                z,
            },
        )
    };
    result.map_or_else(
        |error| translate_error(Some(world), &error),
        |()| DisunityResult::ok(),
    )
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
    let result = {
        let mut store = crate::lock_store();
        crate::commands::apply_immediate(
            &mut store,
            world,
            DisunityCommand {
                object,
                kind: command_kind::SET_ROTATION_Y,
                x: radians,
                y: 0.0,
                z: 0.0,
            },
        )
    };
    result.map_or_else(
        |error| translate_error(Some(world), &error),
        |()| DisunityResult::ok(),
    )
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
        Err(_) => DisunityResult::invalid_handle(),
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
    if (buffer.is_null() && capacity != 0) || out_len.is_null() {
        return DisunityResult::invalid_argument();
    }
    let message = crate::store::last_error(world);
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
                if !bytes.is_empty() {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
                }
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
            |reason| translate_error(Some(world), &reason),
            |()| DisunityResult::ok(),
        )
    }
}

/// Reads authoritative native values without constructing a managed mirror.
/// # Safety
/// `output` is writable for one state row.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_gameobject_read(
    world: u64,
    object: u64,
    output: *mut crate::DisunityObjectState,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let store = crate::lock_store();
    let Ok(session) = store.session(world) else {
        return DisunityResult::invalid_handle();
    };
    let Ok(native) = session.object(object) else {
        return DisunityResult::invalid_handle();
    };
    let value = match crate::views::observe(session, native) {
        Ok(value) => value,
        Err(_) => return DisunityResult::invalid_handle(),
    };
    unsafe {
        *output = crate::DisunityObjectState {
            object,
            x: value.position[0],
            y: value.position[1],
            z: value.position[2],
            rotation_y_radians: value.rotation_y_radians,
            material_slot: value.material_slot,
        }
    };
    DisunityResult::ok()
}
