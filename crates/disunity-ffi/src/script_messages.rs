//! Reports which per-frame messages a script type defines.
use crate::{DisunityResult, callbacks::mutate};
use std::ffi::CStr;
use worldrunner_world_simulation::ScriptMessages;

/// Copies a NUL-terminated UTF-8 string, or `None` when the pointer is null or the text invalid.
unsafe fn text(pointer: *const u8) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer.cast()) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

/// Records the per-frame messages (bit 0 `FixedUpdate`, bit 1 `Update`, bit 2 `LateUpdate`) that
/// the script type `type_name` in `assembly` defines. Scripts of the type, present or added
/// later, then receive only those messages. Types never reported receive all three.
/// # Safety
/// `assembly` and `type_name` are NUL-terminated strings, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_script_type_messages(
    world: u64,
    assembly: *const u8,
    type_name: *const u8,
    messages: u32,
) -> DisunityResult {
    let (Some(assembly), Some(type_name), Some(messages)) = (
        unsafe { text(assembly) },
        unsafe { text(type_name) },
        ScriptMessages::from_bits(messages),
    ) else {
        return DisunityResult::invalid_argument();
    };
    mutate(world, |session| {
        session
            .native
            .set_script_type_messages(&assembly, &type_name, messages)
            .map_err(crate::errors::AbiError::from)
    })
}
