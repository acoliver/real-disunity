//! Project-owned named keyboard axis registration and phase reads.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError};
use worldrunner_world_simulation::{AxisBinding, AxisId, InputAxis, InputPhase, KeyCode};

/// Missing buttons use 65536, outside the native key code range.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityInputAxis {
    pub negative: u32,
    pub positive: u32,
    pub alt_negative: u32,
    pub alt_positive: u32,
    pub gravity: f32,
    pub sensitivity: f32,
    pub dead: f32,
    /// Bit 0: snap; bit 1: invert.
    pub flags: u32,
}
fn keys(primary: u32, alternate: u32) -> Result<Vec<KeyCode>, AbiError> {
    [primary, alternate]
        .into_iter()
        .filter(|&key| key != 65536)
        .map(|key| {
            u16::try_from(key)
                .map(KeyCode)
                .map_err(|_| "invalid axis key".into())
        })
        .collect()
}
unsafe fn name(bytes: *const u8, length: u32) -> Result<String, AbiError> {
    if bytes.is_null() || !(1..=1024).contains(&length) {
        return Err("invalid axis name".into());
    }
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(bytes, length as usize) })
        .map(str::to_owned)
        .map_err(|_| "invalid UTF-8 axis name".into())
}

/// # Safety
/// `bytes` spans `length` readable bytes; `config` points to one readable record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_input_axis_register(
    world: u64,
    id: u32,
    bytes: *const u8,
    length: u32,
    config: *const DisunityInputAxis,
) -> DisunityResult {
    if config.is_null() || id > u32::from(u16::MAX) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let config = unsafe { *config };
        if config.flags > 3 {
            return Err("invalid axis flags".into());
        }
        let axis = InputAxis {
            name: unsafe { name(bytes, length)? },
            binding: AxisBinding {
                negative: keys(config.negative, config.alt_negative)?,
                positive: keys(config.positive, config.alt_positive)?,
                invert: config.flags & 2 != 0,
            },
            gravity: config.gravity,
            sensitivity: config.sensitivity,
            dead: config.dead,
            snap: config.flags & 1 != 0,
        };
        session
            .native
            .register_input_axis(AxisId(u16::try_from(id).expect("checked axis")), axis)
            .map_err(AbiError::from)
    })
}

/// # Safety
/// `bytes` spans `length` readable bytes; `output` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_input_axis_read(
    world: u64,
    phase: u32,
    bytes: *const u8,
    length: u32,
    raw: u32,
    output: *mut f32,
) -> DisunityResult {
    if output.is_null() || phase > 1 || raw > 1 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let name = unsafe { name(bytes, length)? };
        let phase = if phase == 1 {
            InputPhase::Fixed
        } else {
            InputPhase::Dynamic
        };
        let value = session
            .native
            .input_axis(phase, &name, raw == 1)
            .map_err(AbiError::from)?;
        unsafe { *output = value };
        Ok(())
    })
}

/// # Safety
/// `bytes` spans `length` readable bytes; `output` is writable for one ID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_input_axis_id(
    world: u64,
    bytes: *const u8,
    length: u32,
    output: *mut u32,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let id = session
            .native
            .input_axis_id(&unsafe { name(bytes, length)? })
            .map_err(AbiError::from)?;
        unsafe { *output = u32::from(id.0) };
        Ok(())
    })
}
