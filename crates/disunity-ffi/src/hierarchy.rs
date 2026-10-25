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
                .iter()
                .find(|(_, value)| **value == parent)
                .map(|(&id, _)| id)
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
