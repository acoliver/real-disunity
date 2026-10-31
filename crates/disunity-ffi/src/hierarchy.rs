//! Hierarchy mutations delegate to the single native world.
use crate::{
    abi::DisunityResult,
    scheduling::{DisunityAdvance, call},
    store::lock_store,
};
use worldrunner_world_simulation::ParentSpace;

/// # Safety
/// Output is writable for one callback record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_parent(
    world: u64,
    object: u64,
    parent: u64,
    space: u32,
    output: *mut DisunityAdvance,
) -> DisunityResult {
    let space = match space {
        0 => ParentSpace::KeepLocal,
        1 => ParentSpace::KeepWorld,
        _ => return DisunityResult::invalid_argument(),
    };
    call(world, output, |session| {
        let object = session.object(object)?;
        let parent = if parent == 0 {
            None
        } else {
            Some(session.object(parent)?)
        };
        let mutation = session
            .native
            .set_parent(object, parent, space)
            .map_err(crate::errors::AbiError::from)?;
        session
            .native
            .next_lifecycle(mutation)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// Output is writable for one encoded object identity, zero for no parent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_get_parent(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let result: Result<u64, crate::errors::AbiError> = (|| {
        let store = lock_store();
        let session = store.session(world)?;
        let parent = session
            .native
            .parent(session.object(object)?)
            .map_err(crate::errors::AbiError::from)?;
        match parent {
            None => Ok(0),
            Some(parent) => session
                .objects
                .id_of(parent)
                .ok_or_else(|| "parent has no ABI identity".into()),
        }
    })();
    match result {
        Ok(parent) => {
            unsafe {
                *output = parent;
            }
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(Some(world), &error),
    }
}

/// # Safety
/// Output is writable for one callback record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_gameobject_set_active(
    world: u64,
    object: u64,
    active: u32,
    output: *mut DisunityAdvance,
) -> DisunityResult {
    if active > 1 {
        return DisunityResult::invalid_argument();
    }
    call(world, output, |session| {
        let object = session.object(object)?;
        let mutation = session
            .native
            .set_active(object, active != 0)
            .map_err(crate::errors::AbiError::from)?;
        session
            .native
            .next_lifecycle(mutation)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `output` is writable for one flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_script_enabled(
    world: u64,
    identity: u64,
    output: *mut u32,
) -> crate::DisunityResult {
    if output.is_null() {
        return crate::DisunityResult::invalid_argument();
    }
    crate::callbacks::mutate(world, |s| {
        let script = s
            .dispatch
            .scripts
            .iter()
            .find_map(|(&handle, &id)| (id == identity).then_some(handle))
            .ok_or("script binding missing")?;
        let value = s
            .native
            .script_enabled(script)
            .map_err(crate::errors::AbiError::from)?;
        unsafe { *output = u32::from(value) };
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one advance record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_script_set_enabled(
    world: u64,
    identity: u64,
    enabled: u32,
    output: *mut crate::scheduling::DisunityAdvance,
) -> crate::DisunityResult {
    if enabled > 1 {
        return crate::DisunityResult::invalid_argument();
    }
    crate::scheduling::call(world, output, |s| {
        let script = s
            .dispatch
            .scripts
            .iter()
            .find_map(|(&handle, &id)| (id == identity).then_some(handle))
            .ok_or("script binding missing")?;
        let mutation = s
            .native
            .set_script_enabled(script, enabled == 1)
            .map_err(crate::errors::AbiError::from)?;
        s.native
            .next_lifecycle(mutation)
            .map_err(crate::errors::AbiError::from)
    })
}

/// Writes the identities of the children of `object` to `buffer`, in no particular order.
/// `written` receives the number of children. When `capacity` is smaller the call returns
/// BUDGET and writes no identities, so the caller can retry with a buffer of that size.
/// Children without an ABI identity (objects still being constructed) are not listed.
///
/// # Safety
/// `buffer` is writable for `capacity` identities and `written` for one count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_children(
    world: u64,
    object: u64,
    buffer: *mut u64,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (buffer.is_null() && capacity != 0) {
        return DisunityResult::invalid_argument();
    }
    let result: Result<Vec<u64>, crate::errors::AbiError> = (|| {
        let store = lock_store();
        let session = store.session(world)?;
        let children = session
            .native
            .children(session.object(object)?)
            .map_err(crate::errors::AbiError::from)?;
        Ok(children
            .into_iter()
            .filter_map(|child| session.objects.id_of(child))
            .collect())
    })();
    match result {
        Ok(ids) => {
            unsafe { *written = ids.len() as u32 };
            if ids.len() > capacity as usize {
                return DisunityResult {
                    code: crate::abi::result_code::BUDGET,
                    detail: 0,
                };
            }
            if !ids.is_empty() {
                unsafe { std::ptr::copy_nonoverlapping(ids.as_ptr(), buffer, ids.len()) };
            }
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(Some(world), &error),
    }
}
