//! The work a scriptable render context records into the frame's offscreen list: camera setup and
//! culling results. Each call validates its arguments and forwards one record; the world checks the rest
//! and keeps the order.
use crate::{
    DisunityResult, callbacks::mutate, graphics_commands::DisunityTargetRef, physics::component,
};
use worldrunner_world_simulation::{
    WorldError,
    graphics::{
        CameraBuffers, DrawTopology, MAX_COLOR_TARGETS, MAX_CULL_PLANES, MaterialId, OffscreenWork,
        RenderTextureId,
    },
};

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

/// Records that draws from here use the view and projection of `camera` (`matrices` holds the view
/// and then the projection, 16 floats each, column-major) and go to `target`. With `buffer_count`
/// above 0 the camera's target is the camera target and its buffers are the color buffers of the
/// `buffer_count` render textures at `buffer_colors` (in output order) and the depth buffer of
/// `buffer_depth` (0 for none), as the camera had them when the script made the call; `target` is
/// then the camera target.
///
/// # Safety
/// `matrices` is readable for 32 floats; `target` is readable for one record; `buffer_colors` is
/// readable for `buffer_count` ids, or null when the count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_setup_camera(
    world: u64,
    camera: u64,
    matrices: *const f32,
    target: *const DisunityTargetRef,
    buffer_colors: *const u64,
    buffer_count: u32,
    buffer_depth: u64,
) -> DisunityResult {
    let count = buffer_count as usize;
    if matrices.is_null()
        || target.is_null()
        || count > MAX_COLOR_TARGETS
        || (count > 0 && buffer_colors.is_null())
        || (count == 0 && buffer_depth != 0)
    {
        return DisunityResult::invalid_argument();
    }
    let colors: Vec<RenderTextureId> = if count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(buffer_colors, count) }
            .iter()
            .map(|bits| RenderTextureId::from_bits(*bits))
            .collect()
    };
    let depth = (buffer_depth != 0).then(|| RenderTextureId::from_bits(buffer_depth));
    let (mut held, target) = ([[0.0_f32; 16]; 2], unsafe { *target });
    unsafe {
        std::ptr::copy_nonoverlapping(matrices, held[0].as_mut_ptr(), 16);
        std::ptr::copy_nonoverlapping(matrices.add(16), held[1].as_mut_ptr(), 16);
    }
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::SetupCamera {
            camera: component(s, camera)?,
            view: held[0],
            projection: held[1],
            target: target.to_native()?,
            buffers: if colors.is_empty() {
                None
            } else {
                Some(CameraBuffers::new(&colors, depth).map_err(WorldError::from)?)
            },
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

/// Records the rectangle, in pixels of the bound targets from the bottom left, that the draws after
/// it cover. A value that is not finite and a negative width or height are refused.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_set_viewport(
    world: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> DisunityResult {
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::SetViewport {
            rect: [x, y, width, height],
        })?;
        Ok(())
    })
}

/// Records the rectangle, in pixels of the bound targets from the bottom left, that clips the draws
/// after it until a disable, a new target or a camera setup. A value that is not finite and a
/// negative width or height are refused.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_enable_scissor(
    world: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> DisunityResult {
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::EnableScissor {
            rect: [x, y, width, height],
        })?;
        Ok(())
    })
}

/// Records the end of the clip `disunity_offscreen_enable_scissor` set.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_disable_scissor(world: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::DisableScissor)?;
        Ok(())
    })
}

/// Records the depth bias of the draws after it: `bias` is the constant offset and `slope_bias` the
/// factor of the triangle's depth slope. A value that is not finite is refused.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_set_depth_bias(
    world: u64,
    bias: f32,
    slope_bias: f32,
) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .record_offscreen(OffscreenWork::SetDepthBias { bias, slope_bias })?;
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
    unsafe { draw_mesh(world, (mesh, matrix), material, (submesh, pass, 1)) }
}

/// Records the draw of `disunity_offscreen_draw_mesh` that makes `instances` instances of the
/// sub-mesh. A count of 0 is a draw of nothing that keeps its place among the frame's records.
///
/// # Safety
/// `matrix` is readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_draw_mesh_instanced(
    world: u64,
    mesh: u64,
    matrix: *const f32,
    material: u64,
    submesh: u32,
    pass: i32,
    instances: u32,
) -> DisunityResult {
    unsafe { draw_mesh(world, (mesh, matrix), material, (submesh, pass, instances)) }
}

/// # Safety
/// `matrix` is readable for 16 floats.
unsafe fn draw_mesh(
    world: u64,
    (mesh, matrix): (u64, *const f32),
    material: u64,
    (submesh, pass, instances): (u32, i32, u32),
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
            instances,
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

/// Records a request to fill mip levels 1 and up of the render texture `texture` from its level 0,
/// after the work recorded before it. The world refuses a dead texture and one with a single mip
/// level.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_offscreen_generate_mips(world: u64, texture: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native.record_offscreen(OffscreenWork::GenerateMips {
            texture: RenderTextureId::from_bits(texture),
        })?;
        Ok(())
    })
}
