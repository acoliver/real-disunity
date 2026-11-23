//! The work command buffers record into the frame's offscreen list: render targets, clears, a
//! global texture, and temporary render textures. Each call validates its arguments and forwards
//! one record; the world checks the rest and keeps the order. Global floats, vectors, matrices,
//! arrays and keywords are direct writes of the world's global state (`graphics_properties`);
//! the world puts a record for each in front of the next one.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics::{DisunityRenderTexture, text},
    graphics_properties::DisunityTextureSlot,
};
use worldrunner_world_simulation::graphics::{
    MAX_COLOR_TARGETS, OffscreenWork, PropertyValue, RenderTextureId, TargetRef,
};

const TARGET_RENDER: u32 = 0;
const TARGET_TEMPORARY: u32 = 1;
const TARGET_CAMERA: u32 = 2;
const TARGET_NONE: u32 = 3;

const CLEAR_COLOR: u32 = 1;
const CLEAR_DEPTH: u32 = 2;
const CLEAR_STENCIL: u32 = 4;

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
    pub(crate) fn to_native(self) -> Result<TargetRef, AbiError> {
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

/// Records a global texture to set when the host reaches the record. The slot names the texture,
/// by the kinds `disunity_shader_texture_write` takes and also a temporary render texture, which
/// has no world identity that outlives its frame and so cannot be written to the global state.
/// Every other global value is a direct write of the global state (`disunity_shader_*`); the
/// world orders that write in front of the next record.
///
/// # Safety
/// `name` spans `length` readable bytes; `slot` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_set_global_texture(
    world: u64,
    name: *const u8,
    length: u32,
    slot: *const DisunityTextureSlot,
) -> DisunityResult {
    if slot.is_null() {
        return DisunityResult::invalid_argument();
    }
    let slot = unsafe { *slot };
    record(world, || {
        let name = unsafe { text(name, length) }?;
        Ok(OffscreenWork::SetGlobal(PropertyValue::Texture(
            name,
            slot.to_native()?,
        )))
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
