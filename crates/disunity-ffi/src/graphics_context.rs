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
use worldrunner_world_simulation::graphics::{
    DrawTopology, MAX_CULL_PLANES, MaterialId, OffscreenWork,
};

pub const KIND_CULL: u32 = 10;
pub const KIND_SETUP_CAMERA: u32 = 11;
pub const KIND_SET_VIEW_PROJECTION: u32 = 15;
pub const KIND_DRAW_MESH: u32 = 16;
pub const KIND_DRAW_PROCEDURAL: u32 = 17;

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

/// Records the view and projection that the draws after it use: 16 floats each, column-major,
/// as a camera setup carries them. It names no camera and leaves the bound targets as they are.
///
/// # Safety
/// `view` and `projection` are readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_set_view_projection(
    world: u64,
    view: *const f32,
    projection: *const f32,
) -> DisunityResult {
    if view.is_null() || projection.is_null() {
        return DisunityResult::invalid_argument();
    }
    let mut matrices = [[0.0_f32; 16]; 2];
    unsafe {
        std::ptr::copy_nonoverlapping(view, matrices[0].as_mut_ptr(), 16);
        std::ptr::copy_nonoverlapping(projection, matrices[1].as_mut_ptr(), 16);
    }
    mutate(world, |s| {
        s.native
            .record_offscreen(OffscreenWork::SetViewProjection {
                view: matrices[0],
                projection: matrices[1],
            })?;
        Ok(())
    })
}

/// Records a draw of the stored mesh `mesh` (see `disunity_mesh_store`) with `matrix` (16 floats,
/// column-major, from the mesh's coordinates to the world), the `material`'s `pass` (-1 for every
/// pass) and the mesh's sub-mesh `submesh`, into the bound targets with the current view and
/// projection. The values written to the draw scope (3) since the last call are the draw's own
/// property block; the call takes them whether or not the draw is recorded, so they never reach a
/// later draw. An identity nothing stored is refused.
///
/// # Safety
/// `matrix` is readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_draw_mesh(
    world: u64,
    mesh: u64,
    matrix: *const f32,
    material: u64,
    submesh: u32,
    pass: i32,
) -> DisunityResult {
    mutate(world, |s| {
        let block = std::mem::take(&mut s.draw_block);
        if matrix.is_null() {
            return Err("a mesh draw needs a matrix".into());
        }
        let mut values = [0.0_f32; 16];
        unsafe { std::ptr::copy_nonoverlapping(matrix, values.as_mut_ptr(), 16) };
        if !s.meshes.contains_key(&mesh) {
            return Err("a mesh draw names a mesh identity nothing stored".into());
        }
        s.native.record_offscreen(OffscreenWork::DrawMesh {
            mesh,
            matrix: values,
            material: MaterialId::from_bits(material),
            submesh,
            pass,
            block: (!block.is_empty()).then(|| Box::new(block)),
        })?;
        Ok(())
    })
}

/// Records a draw of `vertex_count` generated vertices, `instance_count` times, with `matrix`
/// (16 floats, column-major) and the `material`'s `pass` (-1 for every pass), into the bound
/// targets with the current view and projection. `topology` is 0 for a triangle list and 1 for a
/// line list; any other value and a negative count are refused.
///
/// # Safety
/// `matrix` is readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_draw_procedural(
    world: u64,
    matrix: *const f32,
    material: u64,
    pass: i32,
    topology: u32,
    vertex_count: i32,
    instance_count: i32,
) -> DisunityResult {
    let topology = match topology {
        0 => DrawTopology::TriangleList,
        1 => DrawTopology::LineList,
        _ => return DisunityResult::invalid_argument(),
    };
    let (Ok(vertex_count), Ok(instance_count)) =
        (u32::try_from(vertex_count), u32::try_from(instance_count))
    else {
        return DisunityResult::invalid_argument();
    };
    if matrix.is_null() {
        return DisunityResult::invalid_argument();
    }
    let mut values = [0.0_f32; 16];
    unsafe { std::ptr::copy_nonoverlapping(matrix, values.as_mut_ptr(), 16) };
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::DrawProcedural {
            matrix: values,
            material: MaterialId::from_bits(material),
            pass,
            topology,
            vertex_count,
            instance_count,
        })?;
        Ok(())
    })
}

/// The kind a taken context record has, or `None` for other work.
pub(crate) fn context_kind(work: &OffscreenWork) -> Option<u32> {
    match work {
        OffscreenWork::Cull { .. } => Some(KIND_CULL),
        OffscreenWork::SetupCamera { .. } => Some(KIND_SETUP_CAMERA),
        OffscreenWork::SetViewProjection { .. } => Some(KIND_SET_VIEW_PROJECTION),
        OffscreenWork::DrawMesh { .. } => Some(KIND_DRAW_MESH),
        OffscreenWork::DrawProcedural { .. } => Some(KIND_DRAW_PROCEDURAL),
        other => readback_kind(other),
    }
}
