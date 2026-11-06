//! The work command buffers record into the frame's offscreen list: render targets, clears,
//! global values and keywords, and temporary render textures. Each call validates its arguments
//! and forwards one record; the world checks the rest and keeps the order.
//!
//! Taking the frame's offscreen work (see [`crate::graphics_offscreen`]) gives these records a
//! kind from [`KIND_SET_RENDER_TARGET`] up and a token, from 1, that [`disunity_offscreen_detail`]
//! reads the record's bytes by. A token is valid until the next call that takes work.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics::{DisunityRenderTexture, text},
    graphics_properties::DisunityTextureSlot,
};
use worldrunner_world_simulation::graphics::{
    MAX_COLOR_TARGETS, OffscreenWork, PropertyValue, RenderTextureId, TargetRef,
    snapshot_wire::encode_work,
};

pub(crate) const KIND_SET_RENDER_TARGET: u32 = 4;
pub(crate) const KIND_CLEAR_RENDER_TARGET: u32 = 5;
pub(crate) const KIND_SET_GLOBAL: u32 = 6;
pub(crate) const KIND_SET_GLOBAL_KEYWORD: u32 = 7;
pub(crate) const KIND_ALLOCATE_TEMPORARY: u32 = 8;
pub(crate) const KIND_RELEASE_TEMPORARY: u32 = 9;

const TARGET_RENDER: u32 = 0;
const TARGET_TEMPORARY: u32 = 1;
const TARGET_CAMERA: u32 = 2;
const TARGET_NONE: u32 = 3;

const CLEAR_COLOR: u32 = 1;
const CLEAR_DEPTH: u32 = 2;
const CLEAR_STENCIL: u32 = 4;

const GLOBAL_FLOAT: u32 = 0;
const GLOBAL_INT: u32 = 1;
const GLOBAL_VECTOR: u32 = 2;
const GLOBAL_MATRIX: u32 = 3;
const GLOBAL_TEXTURE: u32 = 4;

/// A target. `kind` is 0 for a render texture (`render` is its id), 1 for a temporary render
/// texture (`name_id` is the property name id it was allocated under), 2 for the camera's
/// target and 3 for none. Fields a kind does not use must be 0.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityTargetRef {
    pub kind: u32,
    pub name_id: i32,
    pub render: u64,
}

impl DisunityTargetRef {
    fn to_native(self) -> Result<TargetRef, AbiError> {
        match (self.kind, self.name_id, self.render) {
            (TARGET_RENDER, 0, id) if id != 0 => {
                Ok(TargetRef::Render(RenderTextureId::from_bits(id)))
            }
            (TARGET_TEMPORARY, name, 0) => Ok(TargetRef::Temporary(name)),
            (TARGET_CAMERA, 0, 0) => Ok(TargetRef::CameraTarget),
            (TARGET_NONE, 0, 0) => Ok(TargetRef::None),
            _ => Err(
                "a target is a render texture (0), a temporary (1), the camera target (2) or none (3), with its other fields zero"
                    .into(),
            ),
        }
    }
}

fn record(world: u64, work: impl FnOnce() -> Result<OffscreenWork, AbiError>) -> DisunityResult {
    mutate(world, |s| {
        s.native.record_offscreen(work()?)?;
        Ok(())
    })
}

/// Records that draws from here go to `color_count` color targets and an optional `depth`
/// target (null for none), at `mip`, cube `face` (-1 for every face) and depth `slice` (-1 for
/// every slice).
///
/// # Safety
/// `colors` is readable for `color_count` records, or null when the count is zero; `depth` is
/// readable for one record, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_set_render_target(
    world: u64,
    colors: *const DisunityTargetRef,
    color_count: u32,
    depth: *const DisunityTargetRef,
    mip: u32,
    face: i32,
    slice: i32,
) -> DisunityResult {
    let count = color_count as usize;
    if count > MAX_COLOR_TARGETS || (count > 0 && colors.is_null()) {
        return DisunityResult::invalid_argument();
    }
    record(world, || {
        let given: &[DisunityTargetRef] = if count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(colors, count) }
        };
        Ok(OffscreenWork::SetRenderTarget {
            color: given
                .iter()
                .map(|target| target.to_native())
                .collect::<Result<_, _>>()?,
            depth: if depth.is_null() {
                None
            } else {
                Some(unsafe { *depth }.to_native()?)
            },
            mip,
            face,
            slice,
        })
    })
}

/// Records a clear of the bound targets. `flags` has 1 for the color, 2 for the depth and 4 for
/// the stencil; `value` is 4 floats.
///
/// # Safety
/// `value` is readable for 4 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_clear_render_target(
    world: u64,
    flags: u32,
    value: *const f32,
    depth_value: f32,
    stencil_value: u32,
) -> DisunityResult {
    if value.is_null() || flags & !(CLEAR_COLOR | CLEAR_DEPTH | CLEAR_STENCIL) != 0 {
        return DisunityResult::invalid_argument();
    }
    let mut color = [0.0; 4];
    unsafe { std::ptr::copy_nonoverlapping(value, color.as_mut_ptr(), color.len()) };
    record(world, || {
        Ok(OffscreenWork::ClearRenderTarget {
            color: flags & CLEAR_COLOR != 0,
            depth: flags & CLEAR_DEPTH != 0,
            stencil: flags & CLEAR_STENCIL != 0,
            value: color,
            depth_value,
            stencil_value,
        })
    })
}

/// Records a global shader value to set when the host reaches the record. `kind` is 0 for a
/// float (`floats` holds 1), 1 for an integer (`int_value`), 2 for a vector (4 floats), 3 for a
/// matrix (16 floats, column-major) and 4 for a texture (`slot`).
///
/// # Safety
/// `name` spans `length` readable bytes; `floats` is readable for the floats the kind needs;
/// `slot` is readable for one record when the kind is 4.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_set_global(
    world: u64,
    kind: u32,
    name: *const u8,
    length: u32,
    floats: *const f32,
    int_value: i32,
    slot: *const DisunityTextureSlot,
) -> DisunityResult {
    record(world, || {
        let name = unsafe { text(name, length) }?;
        let value =
            match kind {
                GLOBAL_FLOAT => PropertyValue::Float(name, unsafe { floats_of::<1>(floats) }?[0]),
                GLOBAL_INT => PropertyValue::Int(name, int_value),
                GLOBAL_VECTOR => PropertyValue::Vector(name, unsafe { floats_of::<4>(floats) }?),
                GLOBAL_MATRIX => PropertyValue::Matrix(name, unsafe { floats_of::<16>(floats) }?),
                GLOBAL_TEXTURE if !slot.is_null() => {
                    PropertyValue::Texture(name, unsafe { *slot }.to_native()?)
                }
                GLOBAL_TEXTURE => return Err("a texture value needs a slot".into()),
                _ => return Err(
                    "a global value is a float (0), int (1), vector (2), matrix (3) or texture (4)"
                        .into(),
                ),
            };
        Ok(OffscreenWork::SetGlobal(value))
    })
}

/// Reads `N` floats.
///
/// # Safety
/// `floats` is readable for `N` floats when it is not null.
unsafe fn floats_of<const N: usize>(floats: *const f32) -> Result<[f32; N], AbiError> {
    if floats.is_null() {
        return Err("a float value needs its floats".into());
    }
    let mut out = [0.0; N];
    unsafe { std::ptr::copy_nonoverlapping(floats, out.as_mut_ptr(), N) };
    Ok(out)
}

/// Records a global shader keyword to turn on (`enabled` 1) or off (0) when the host reaches
/// the record.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_set_global_keyword(
    world: u64,
    name: *const u8,
    length: u32,
    enabled: u32,
) -> DisunityResult {
    if enabled > 1 {
        return DisunityResult::invalid_argument();
    }
    record(world, || {
        Ok(OffscreenWork::SetGlobalKeyword {
            name: unsafe { text(name, length) }?,
            enabled: enabled == 1,
        })
    })
}

/// Records that a temporary render texture of this description exists under `name_id` from
/// here on.
///
/// # Safety
/// `desc` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_allocate_temporary(
    world: u64,
    name_id: i32,
    desc: *const DisunityRenderTexture,
) -> DisunityResult {
    if desc.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    record(world, || {
        Ok(OffscreenWork::AllocateTemporary {
            name_id,
            desc: desc.to_native()?,
        })
    })
}

/// Records the release of the temporary render texture under `name_id`.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_release_temporary(world: u64, name_id: i32) -> DisunityResult {
    record(world, || Ok(OffscreenWork::ReleaseTemporary { name_id }))
}

/// The kind a taken command record has, or `None` for other work.
pub(crate) fn command_kind(work: &OffscreenWork) -> Option<u32> {
    match work {
        OffscreenWork::SetRenderTarget { .. } => Some(KIND_SET_RENDER_TARGET),
        OffscreenWork::ClearRenderTarget { .. } => Some(KIND_CLEAR_RENDER_TARGET),
        OffscreenWork::SetGlobal(_) => Some(KIND_SET_GLOBAL),
        OffscreenWork::SetGlobalKeyword { .. } => Some(KIND_SET_GLOBAL_KEYWORD),
        OffscreenWork::AllocateTemporary { .. } => Some(KIND_ALLOCATE_TEMPORARY),
        OffscreenWork::ReleaseTemporary { .. } => Some(KIND_RELEASE_TEMPORARY),
        _ => None,
    }
}

/// The bytes of a command record, as the world's snapshot encoding writes it.
pub(crate) fn detail_of(work: &OffscreenWork) -> Result<Vec<u8>, AbiError> {
    encode_work(work).map_err(|error| AbiError::Message(error.to_string()))
}

/// Reads the encoded bytes of a command record by the token its take gave it (see
/// `worldrunner_world_simulation::graphics::snapshot_wire::decode_work`). Writes the length to
/// `length`; with a zero `capacity` that is all it does, and a smaller `capacity` is rejected.
///
/// # Safety
/// `buffer` is writable for `capacity` bytes, or null when `capacity` is zero; `length` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_detail(
    world: u64,
    token: u32,
    buffer: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> DisunityResult {
    if length.is_null() || (capacity > 0 && buffer.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let index = usize::try_from(token)
            .ok()
            .and_then(|token| token.checked_sub(1))
            .ok_or("a detail token is nonzero")?;
        let held = s
            .work_details
            .get(index)
            .ok_or("the detail token names no command of the last take")?;
        let needed = u32::try_from(held.len()).map_err(|_| "detail overflow")?;
        unsafe { *length = needed };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < needed {
            return Err("the buffer is too small for the record".into());
        }
        unsafe { std::ptr::copy_nonoverlapping(held.as_ptr(), buffer, held.len()) };
        Ok(())
    })
}
