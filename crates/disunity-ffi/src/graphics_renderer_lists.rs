//! Renderer lists through the ABI: a script turns a culling result into a list of what a render
//! pass draws (or of one light's shadow casters) and later draws the list from a command buffer.
//! Each call validates its pointers, decodes its arguments and forwards one record; the world
//! checks the rest and keeps the order.
//!
//! Taking the frame's offscreen work (see [`crate::graphics_offscreen`]) gives a list creation
//! the kind [`KIND_CREATE_RENDERER_LIST`] and a draw the kind [`KIND_DRAW_RENDERER_LIST`], each
//! with a token that `disunity_offscreen_detail` reads the record's encoded bytes by (see
//! [`crate::graphics_commands`]).
use crate::{DisunityResult, callbacks::mutate, errors::AbiError};
use worldrunner_world_simulation::graphics::{MaterialId, OffscreenWork, RendererListDesc};

pub const KIND_CREATE_RENDERER_LIST: u32 = 13;
pub const KIND_DRAW_RENDERER_LIST: u32 = 14;

/// The fields of a list that draws a culling result's renderers. `tags_len` is the byte length of
/// the pass tag names (`LightMode` values) that the call's `tags` pointer holds, UTF-8 names joined
/// by zero bytes. `override_material` is 0 for none. The queue range is inclusive.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityDrawRendererList {
    pub cull: u64,
    pub override_material: u64,
    pub queue_min: i32,
    pub queue_max: i32,
    pub layer_mask: u32,
    pub rendering_layer_mask: u32,
    pub sorting: u32,
    pub per_object: u32,
    pub override_pass: i32,
    pub tags_len: u32,
}

/// The fields of a list of one light's shadow casters. `plane_count` is the number of split
/// culling planes (4 floats each) that the call's `planes` pointer holds. The split culling sphere
/// is the centre in `sphere_x`, `sphere_y` and `sphere_z` and the radius in `sphere_radius`.
/// `objects_filter` is 0 for all objects, 1 for dynamic ones and 2 for static ones; `projection`
/// is 0 for unknown, 1 for perspective and 2 for orthographic.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityShadowRendererList {
    pub sphere_x: f32,
    pub sphere_y: f32,
    pub sphere_z: f32,
    pub sphere_radius: f32,
    pub light_index: i32,
    pub plane_count: u32,
    pub objects_filter: u32,
    pub projection: u32,
    pub cull: u64,
}

/// The kind a taken renderer list record has, or `None` for other work.
pub(crate) fn renderer_list_kind(work: &OffscreenWork) -> Option<u32> {
    match work {
        OffscreenWork::CreateRendererList { .. } => Some(KIND_CREATE_RENDERER_LIST),
        OffscreenWork::DrawRendererList { .. } => Some(KIND_DRAW_RENDERER_LIST),
        _ => None,
    }
}

/// Splits zero-joined UTF-8 names. No bytes mean no names; an empty name between two zero
/// bytes stays in the result for the world to refuse.
fn split_tags(bytes: &[u8]) -> Result<Vec<String>, AbiError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes
        .split(|byte| *byte == 0)
        .map(|name| {
            std::str::from_utf8(name)
                .map(str::to_owned)
                .map_err(|_| "pass tag names must be UTF-8".into())
        })
        .collect()
}

/// Creates a list that draws the renderers the culling result `desc.cull` kept, with the pass tags
/// at `tags` (`desc.tags_len` bytes: names joined by zero bytes, 1 to 16 names), and writes the
/// list id to `list`. A rejected call writes nothing and records nothing.
///
/// # Safety
/// `desc` is readable for one record; `tags` is readable for `desc.tags_len` bytes, or null when
/// the length is zero; `list` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_renderer_list_create_draw(
    world: u64,
    desc: *const DisunityDrawRendererList,
    tags: *const u8,
    list: *mut u64,
) -> DisunityResult {
    if desc.is_null() || list.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    let length = desc.tags_len as usize;
    if length > 0 && tags.is_null() {
        return DisunityResult::invalid_argument();
    }
    let bytes: &[u8] = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(tags, length) }
    };
    mutate(world, |s| {
        let id = s.native.create_renderer_list(RendererListDesc::Draw {
            cull: desc.cull,
            pass_tags: split_tags(bytes)?,
            queue_min: desc.queue_min,
            queue_max: desc.queue_max,
            layer_mask: desc.layer_mask,
            rendering_layer_mask: desc.rendering_layer_mask,
            sorting: desc.sorting,
            per_object: desc.per_object,
            override_material: (desc.override_material != 0)
                .then(|| MaterialId::from_bits(desc.override_material)),
            override_pass: desc.override_pass,
        })?;
        unsafe { *list = id };
        Ok(())
    })
}

/// Creates a list of the shadow casters the culling result `desc.cull` kept for one light, with
/// the `desc.plane_count` split culling planes at `planes` (4 floats each: a normal and a
/// distance), and writes the list id to `list`. A rejected call writes nothing and records
/// nothing.
///
/// # Safety
/// `desc` is readable for one record; `planes` is readable for `desc.plane_count` planes of 4
/// floats, or null when the count is zero; `list` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_renderer_list_create_shadow(
    world: u64,
    desc: *const DisunityShadowRendererList,
    planes: *const f32,
    list: *mut u64,
) -> DisunityResult {
    if desc.is_null() || list.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    let count = desc.plane_count as usize;
    if count > 0 && planes.is_null() {
        return DisunityResult::invalid_argument();
    }
    let floats: &[f32] = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(planes, count * 4) }
    };
    mutate(world, |s| {
        let id = s.native.create_renderer_list(RendererListDesc::Shadow {
            cull: desc.cull,
            light_index: desc.light_index,
            planes: floats
                .chunks_exact(4)
                .map(|plane| [plane[0], plane[1], plane[2], plane[3]])
                .collect(),
            sphere: [
                desc.sphere_x,
                desc.sphere_y,
                desc.sphere_z,
                desc.sphere_radius,
            ],
            objects_filter: desc.objects_filter,
            projection: desc.projection,
        })?;
        unsafe { *list = id };
        Ok(())
    })
}

/// Records that the renderer list `list`, created earlier in the frame, draws into the bound
/// targets from here.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_draw_renderer_list(world: u64, list: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .record_offscreen(OffscreenWork::DrawRendererList { list })?;
        Ok(())
    })
}
