//! Bounded object metadata and transform access to native world storage.
use crate::{DisunityResult, callbacks::mutate};
use glam::{Quat, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTransform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub qx: f32,
    pub qy: f32,
    pub qz: f32,
    pub qw: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
}

/// # Safety
/// `output` is writable for one transform record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_read(
    world: u64,
    object: u64,
    output: *mut DisunityTransform,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let v = s
            .native
            .local_transform(s.object(object)?)
            .map_err(crate::errors::AbiError::from)?;
        unsafe {
            *output = DisunityTransform {
                x: v.translation.x,
                y: v.translation.y,
                z: v.translation.z,
                qx: v.rotation.x,
                qy: v.rotation.y,
                qz: v.rotation.z,
                qw: v.rotation.w,
                sx: v.scale.x,
                sy: v.scale.y,
                sz: v.scale.z,
            }
        };
        Ok(())
    })
}

/// # Safety
/// `value` points to one readable local transform record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_transform_write(
    world: u64,
    object: u64,
    value: *const DisunityTransform,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    mutate(world, |s| {
        s.native
            .set_local_transform(
                s.object(object)?,
                worldrunner_world_simulation::LocalTransform {
                    translation: Vec3::new(v.x, v.y, v.z),
                    rotation: Quat::from_xyzw(v.qx, v.qy, v.qz, v.qw),
                    scale: Vec3::new(v.sx, v.sy, v.sz),
                },
            )
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `bytes` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_object_text_write(
    world: u64,
    object: u64,
    field: u32,
    bytes: *const u8,
    length: u32,
) -> DisunityResult {
    if length > 1024 || (bytes.is_null() && length != 0) || field > 1 {
        return DisunityResult::invalid_argument();
    }
    let input = if length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(bytes, length as usize) }
    };
    let Ok(text) = std::str::from_utf8(input) else {
        return DisunityResult::invalid_argument();
    };
    let text = text.to_owned();
    mutate(world, |s| {
        s.native
            .set_object_text(s.object(object)?, field == 1, text)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `bytes` spans `capacity` writable bytes and `written` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_object_text_read(
    world: u64,
    object: u64,
    field: u32,
    bytes: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (bytes.is_null() && capacity != 0) || field > 1 {
        return DisunityResult::invalid_argument();
    }
    let value = {
        let store = crate::lock_store();
        store.session(world).and_then(|s| {
            s.native
                .object(s.object(object)?)
                .map_err(crate::errors::AbiError::from)
        })
    };
    let object = match value {
        Ok(object) => object,
        Err(error) => return crate::abi::translate_error(Some(world), &error),
    };
    let text = if field == 1 { object.tag } else { object.name };
    unsafe { *written = text.len() as u32 };
    if text.len() > capacity as usize {
        return DisunityResult {
            code: crate::result_code::BUDGET,
            detail: 0,
        };
    }
    if !text.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), bytes, text.len()) };
    }
    DisunityResult::ok()
}

/// # Safety
/// `output` is writable for one bit mask: active self, active hierarchy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_object_flags(
    world: u64,
    object: u64,
    output: *mut u32,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let object = s.object(object)?;
        let own = s
            .native
            .object(object)
            .map_err(crate::errors::AbiError::from)?
            .active;
        let hierarchy = s
            .native
            .active_in_hierarchy(object)
            .map_err(crate::errors::AbiError::from)?;
        unsafe { *output = u32::from(own) | (u32::from(hierarchy) << 1) };
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one fixed-step duration in seconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_fixed_delta(
    world: u64,
    output: *mut f64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        unsafe { *output = s.native.clock().fixed_step.seconds() };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_body_move_position(
    world: u64,
    body: u64,
    x: f32,
    y: f32,
) -> DisunityResult {
    mutate(world, |s| {
        let body = crate::physics::component(s, body)?;
        s.native
            .move_body_position(body, glam::Vec2::new(x, y))
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// Each output is writable for one quaternion coordinate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_rotation_read(
    world: u64,
    object: u64,
    x: *mut f32,
    y: *mut f32,
    z: *mut f32,
    w: *mut f32,
) -> DisunityResult {
    if x.is_null() || y.is_null() || z.is_null() || w.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let q = s
            .native
            .world_rotation(s.object(object)?)
            .map_err(crate::errors::AbiError::from)?;
        unsafe {
            *x = q.x;
            *y = q.y;
            *z = q.z;
            *w = q.w;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_rotation_write(
    world: u64,
    object: u64,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .set_world_rotation(s.object(object)?, Quat::from_xyzw(x, y, z, w))
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `alive` is writable for one lifetime flag. Stale objects return false.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_object_alive(
    world: u64,
    object: u64,
    alive: *mut u32,
) -> DisunityResult {
    if alive.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        unsafe { *alive = u32::from(s.object(object).is_ok()) };
        Ok(())
    })
}

/// # Safety
/// `retained` is writable for one native callback-retention flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_script_retained(
    world: u64,
    identity: u64,
    retained: *mut u32,
) -> DisunityResult {
    if retained.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let pending = s
            .dispatch
            .scripts
            .iter()
            .any(|(&script, &id)| id == identity && s.native.script_retained(script));
        unsafe { *retained = u32::from(pending) };
        Ok(())
    })
}

/// Schedules destruction using scaled world seconds, without running cleanup inline.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_gameobject_destroy_after(
    world: u64,
    object: u64,
    numerator: u64,
    denominator: u64,
) -> DisunityResult {
    mutate(world, |s| {
        let delay = worldrunner_world_simulation::TimeSpan::new(numerator, denominator)
            .map_err(crate::errors::AbiError::from)?;
        let when = if delay == worldrunner_world_simulation::TimeSpan::ZERO {
            worldrunner_world_simulation::DestroyWhen::EndOfFrame
        } else {
            worldrunner_world_simulation::DestroyWhen::AfterScaled(delay)
        };
        s.native
            .destroy(s.object(object)?, when)
            .map_err(crate::errors::AbiError::from)
    })
}
