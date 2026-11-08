//! The work a scriptable render context records into the frame's offscreen list: camera setup and
//! culling results. Each call validates its arguments and forwards one record; the world checks the rest
//! and keeps the order.
//!
//! Taking the frame's offscreen work (see [`crate::graphics_offscreen`]) gives these records a
//! kind from [`KIND_CULL`] up and a token that `disunity_offscreen_detail` reads the record's
//! encoded bytes by (see [`crate::graphics_commands`]).
use crate::{
    DisunityResult, callbacks::mutate, graphics_commands::DisunityTargetRef,
    graphics_readback::readback_kind, physics::component,
};
use worldrunner_world_simulation::graphics::{MAX_CULL_PLANES, OffscreenWork};

pub const KIND_CULL: u32 = 10;
pub const KIND_SETUP_CAMERA: u32 = 11;

/// Records a culling of the world's renderers for `camera` under the handle `id`, which later
/// records of the frame name the result by. The host culls by the layers in `culling_mask`, the
/// `plane_count` planes at `planes` (4 floats each: a unit normal and a distance) and the
/// culling `options` flags.
///
/// # Safety
/// `planes` is readable for `plane_count` planes of 4 floats, or null when the count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_cull(
    world: u64,
    id: u64,
    camera: u64,
    culling_mask: u32,
    planes: *const f32,
    plane_count: u32,
    options: u32,
) -> DisunityResult {
    let count = plane_count as usize;
    if count > MAX_CULL_PLANES || (count > 0 && planes.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let floats: &[f32] = if count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(planes, count * 4) }
        };
        let planes = floats
            .chunks_exact(4)
            .map(|plane| [plane[0], plane[1], plane[2], plane[3]])
            .collect();
        s.native.record_offscreen(OffscreenWork::Cull {
            id,
            camera: component(s, camera)?,
            culling_mask,
            planes,
            options,
        })?;
        Ok(())
    })
}

/// Records that draws from here use the view and projection of `camera` (16 floats each,
/// column-major) and go to `target`.
///
/// # Safety
/// `view` and `projection` are readable for 16 floats; `target` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_setup_camera(
    world: u64,
    camera: u64,
    view: *const f32,
    projection: *const f32,
    target: *const DisunityTargetRef,
) -> DisunityResult {
    if view.is_null() || projection.is_null() || target.is_null() {
        return DisunityResult::invalid_argument();
    }
    let (mut matrices, target) = ([[0.0_f32; 16]; 2], unsafe { *target });
    unsafe {
        std::ptr::copy_nonoverlapping(view, matrices[0].as_mut_ptr(), 16);
        std::ptr::copy_nonoverlapping(projection, matrices[1].as_mut_ptr(), 16);
    }
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::SetupCamera {
            camera: component(s, camera)?,
            view: matrices[0],
            projection: matrices[1],
            target: target.to_native()?,
        })?;
        Ok(())
    })
}

/// The kind a taken context record has, or `None` for other work.
pub(crate) fn context_kind(work: &OffscreenWork) -> Option<u32> {
    match work {
        OffscreenWork::Cull { .. } => Some(KIND_CULL),
        OffscreenWork::SetupCamera { .. } => Some(KIND_SETUP_CAMERA),
        other => readback_kind(other),
    }
}
