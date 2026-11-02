//! The mesh and texture a canvas renderer draws with. The rules live in world-simulation; this
//! file converts records and validates the pointers.
use std::sync::Arc;

use crate::{
    DisunityResult, callbacks::mutate, errors::AbiError, physics::component,
    textures::FIRST_PLAYER_TEXTURE,
};
use worldrunner_world_simulation::{
    ComponentDesc, WorldError,
    ui::{CanvasRenderer, UiGeometry, UiVertex},
};

/// One mesh vertex: its position in the element's local space, its first texture coordinate
/// and its colour.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityUiVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// What a renderer holds to draw: the renderer identity of its texture (0 for none) and the
/// sizes of its mesh.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityUiGraphicInfo {
    pub texture: u64,
    pub vertex_count: u32,
    pub index_count: u32,
}

impl From<DisunityUiVertex> for UiVertex {
    fn from(v: DisunityUiVertex) -> Self {
        Self {
            position: [v.x, v.y, v.z],
            uv: [v.u, v.v],
            color: [v.r, v.g, v.b, v.a],
        }
    }
}

impl From<UiVertex> for DisunityUiVertex {
    fn from(v: UiVertex) -> Self {
        Self {
            x: v.position[0],
            y: v.position[1],
            z: v.position[2],
            u: v.uv[0],
            v: v.uv[1],
            r: v.color[0],
            g: v.color[1],
            b: v.color[2],
            a: v.color[3],
        }
    }
}

fn update(
    world: u64,
    handle: u64,
    change: impl FnOnce(&mut CanvasRenderer) -> Result<(), AbiError>,
) -> DisunityResult {
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::CanvasRenderer(mut renderer) = s.native.component(id)?.desc.clone()
        else {
            return Err("component is not a CanvasRenderer".into());
        };
        change(&mut renderer)?;
        s.native
            .set_component(id, ComponentDesc::CanvasRenderer(renderer))?;
        Ok(())
    })
}

/// Sets the mesh the renderer draws from `vertex_count` vertices and `index_count` triangle
/// indices (the first sub-mesh), or clears it when both counts are zero. Rejects, leaving the
/// renderer unchanged, a null pointer for a nonzero count, an index count that is not a multiple
/// of three, an index past the vertices, a position or texture coordinate that is not finite, a
/// NaN colour, or a mesh over the limits.
///
/// # Safety
/// `vertices` is readable for `vertex_count` records and `indices` for `index_count` values; a
/// pointer may be null when its count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_geometry_write(
    world: u64,
    handle: u64,
    vertices: *const DisunityUiVertex,
    vertex_count: u32,
    indices: *const u32,
    index_count: u32,
) -> DisunityResult {
    if (vertex_count > 0 && vertices.is_null()) || (index_count > 0 && indices.is_null()) {
        return DisunityResult::invalid_argument();
    }
    let vertices: Vec<UiVertex> = if vertex_count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(vertices, vertex_count as usize) }
            .iter()
            .map(|&v| UiVertex::from(v))
            .collect()
    };
    let indices = if index_count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(indices, index_count as usize) }.to_vec()
    };
    update(world, handle, |renderer| {
        renderer.geometry = if vertices.is_empty() && indices.is_empty() {
            None
        } else {
            Some(Arc::new(
                UiGeometry::new(vertices, indices).map_err(WorldError::from)?,
            ))
        };
        Ok(())
    })
}

/// Sets the renderer identity of the texture the renderer draws with; 0 draws plain colour. A
/// nonzero identity must be a player texture identity.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_canvas_renderer_texture_write(
    world: u64,
    handle: u64,
    identity: u64,
) -> DisunityResult {
    update(world, handle, |renderer| {
        if identity != 0 && identity < FIRST_PLAYER_TEXTURE {
            return Err("a canvas renderer draws with a player texture identity or none".into());
        }
        renderer.graphic_texture = identity;
        Ok(())
    })
}

/// Reports the renderer's texture and mesh sizes and copies the vertices when `capacity` is
/// nonzero. A capacity below the vertex count is rejected.
///
/// # Safety
/// `info` is writable for one record; `vertices` is writable for `capacity` records, or null when
/// `capacity` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_geometry_read(
    world: u64,
    handle: u64,
    info: *mut DisunityUiGraphicInfo,
    vertices: *mut DisunityUiVertex,
    capacity: u32,
) -> DisunityResult {
    if info.is_null() || (capacity != 0 && vertices.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::CanvasRenderer(renderer) = &s.native.component(id)?.desc else {
            return Err("component is not a CanvasRenderer".into());
        };
        let mesh = renderer.geometry.as_deref();
        let held = mesh.map_or(0, |m| m.vertices().len());
        let value = DisunityUiGraphicInfo {
            texture: renderer.graphic_texture,
            vertex_count: u32::try_from(held).map_err(|_| "vertex count overflow")?,
            index_count: u32::try_from(mesh.map_or(0, |m| m.indices().len()))
                .map_err(|_| "index count overflow")?,
        };
        unsafe { *info = value };
        if capacity == 0 {
            return Ok(());
        }
        if (capacity as usize) < held {
            return Err("canvas renderer vertex copy capacity is too small".into());
        }
        for (index, vertex) in mesh.iter().flat_map(|m| m.vertices()).enumerate() {
            unsafe { vertices.add(index).write(DisunityUiVertex::from(*vertex)) };
        }
        Ok(())
    })
}
