//! Reading pixels back from a render texture. The script records a request for a rectangle of a
//! render texture; the host executes the work before it, reads the rectangle and writes the
//! texels into the world; the script takes them. The world validates and keeps the answer, and
//! this file checks the pointers and forwards.
//!
//! Taking the frame's offscreen work (see [`crate::graphics_offscreen`]) gives a request record
//! the kind [`KIND_READ_PIXELS`] and a token that `disunity_offscreen_detail` reads the record's
//! encoded bytes by (see [`crate::graphics_commands`]).
use crate::{DisunityResult, callbacks::mutate, graphics_renderer_lists::renderer_list_kind};
use worldrunner_world_simulation::graphics::{OffscreenWork, RenderTextureId};

pub const KIND_READ_PIXELS: u32 = 12;

/// The kind a taken pixel read record has, or `None` for other work.
pub(crate) fn readback_kind(work: &OffscreenWork) -> Option<u32> {
    match work {
        OffscreenWork::ReadPixels { .. } => Some(KIND_READ_PIXELS),
        other => renderer_list_kind(other),
    }
}

/// Records a read of the `width` by `height` texels of mip 0 of a render texture whose bottom
/// left texel is at (`x`, `y`), and writes the id of the request to `request`. The rectangle must
/// be non-empty and inside the texture. The host answers with `width * height * 4` floats (red,
/// green, blue, alpha per texel, bottom row first, as the texture stores them).
///
/// # Safety
/// `request` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_render_texture_read_pixels(
    world: u64,
    texture: u64,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    request: *mut u64,
) -> DisunityResult {
    if request.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = s.native.request_texture_pixels(
            RenderTextureId::from_bits(texture),
            x,
            y,
            width,
            height,
        )?;
        unsafe { *request = id };
        Ok(())
    })
}

/// Writes the texels the host read for a request: `count` floats, which must be the request's
/// `width * height * 4`. A rejected call changes nothing and leaves the request open.
///
/// # Safety
/// `data` is readable for `count` floats, or null when `count` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_render_texture_pixels_write(
    world: u64,
    request: u64,
    data: *const f32,
    count: u32,
) -> DisunityResult {
    if count > 0 && data.is_null() {
        return DisunityResult::invalid_argument();
    }
    let texels: &[f32] = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, count as usize) }
    };
    mutate(world, |s| {
        s.native.write_texture_pixels(request, texels)?;
        Ok(())
    })
}

/// Takes the texels the host wrote for a request into `data` (`capacity` floats) and writes how
/// many floats that is to `written`; taking closes the request. When the host wrote nothing,
/// or the request is unknown or taken already, it writes 0 and succeeds. A `capacity` below the
/// answer is rejected and leaves the answer for a retry.
///
/// # Safety
/// `data` is writable for `capacity` floats, or null when `capacity` is zero; `written` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_render_texture_pixels_take(
    world: u64,
    request: u64,
    data: *mut f32,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (capacity > 0 && data.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let answered = s.native.answered_pixel_floats(request).unwrap_or(0);
        if answered > capacity as usize {
            return Err("the buffer is too small for the pixels".into());
        }
        let floats = s.native.take_texture_pixels(request).unwrap_or_default();
        if !floats.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(floats.as_ptr(), data, floats.len()) };
        }
        unsafe { *written = u32::try_from(floats.len()).map_err(|_| "pixel count overflow")? };
        Ok(())
    })
}
