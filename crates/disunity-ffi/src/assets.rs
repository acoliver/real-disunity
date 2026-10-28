//! Native standalone script asset lifetime and bounded message transfer.
use crate::{DisunityResult, callbacks::mutate};

#[unsafe(no_mangle)]
pub extern "C" fn disunity_asset_create(world: u64, identity: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .create_script_asset(identity)
            .map_err(crate::errors::AbiError::from)
    })
}

/// Makes `identity` receive message 14 once a frame, after `Update` coroutines, where
/// Unity runs work posted to the main thread's synchronization context. `enabled` 0 stops
/// it.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_world_delayed_tasks(
    world: u64,
    identity: u64,
    enabled: u32,
) -> DisunityResult {
    if enabled > 1 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        s.native
            .set_delayed_tasks((enabled == 1).then_some(identity))
            .map_err(crate::errors::AbiError::from)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_asset_destroy(world: u64, identity: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .destroy_script_asset(identity)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `alive` is writable for one flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_asset_alive(
    world: u64,
    identity: u64,
    alive: *mut u32,
) -> DisunityResult {
    if alive.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        unsafe { *alive = u32::from(s.native.script_asset_alive(identity)) };
        Ok(())
    })
}

/// # Safety
/// Output is writable for one work record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_asset_activate(
    world: u64,
    identity: u64,
    output: *mut crate::scheduling::DisunityAdvance,
) -> DisunityResult {
    crate::scheduling::call(world, output, |s| {
        s.native
            .activate_script_asset(identity)
            .map_err(crate::errors::AbiError::from)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_body_set_sleeping(
    world: u64,
    handle: u64,
    sleeping: u32,
) -> DisunityResult {
    if sleeping > 1 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let body = crate::physics::component(s, handle)?;
        s.native
            .set_body_sleeping(body, sleeping == 1)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `sleeping` is writable for one flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_body_sleeping(
    world: u64,
    handle: u64,
    sleeping: *mut u32,
) -> DisunityResult {
    if sleeping.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let body = crate::physics::component(s, handle)?;
        let value = s
            .native
            .body_sleeping(body)
            .map_err(crate::errors::AbiError::from)?;
        unsafe { *sleeping = u32::from(value) };
        Ok(())
    })
}
