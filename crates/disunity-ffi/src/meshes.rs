//! Mesh contents the managed engine hands to the host for meshes that 3D renderers draw. The
//! renderer cannot see managed memory, so the host reads them from here.
use std::collections::BTreeMap;

use crate::{DisunityResult, callbacks::mutate, mesh_channels::StoredChannels};

/// Player meshes take renderer identities from here up, above the player textures (which count up
/// from 2^40), so the two ranges meet only after 2^40 textures; this is the engine's choice.
pub const FIRST_PLAYER_MESH: u64 = 1 << 41;

/// The most vertices one stored mesh holds.
const MAX_VERTICES: u32 = 1 << 24;
/// The most indices one stored mesh holds.
pub(crate) const MAX_INDICES: u32 = 1 << 26;
/// The most sub-meshes one stored mesh holds.
pub(crate) const MAX_SUBMESHES: u32 = 1 << 12;

/// Which attribute channels a stored mesh holds besides positions.
pub const CHANNEL_NORMALS: u32 = 1;
pub const CHANNEL_UV0: u32 = 2;
pub const CHANNEL_COLORS: u32 = 4;
const ALL_CHANNELS: u32 = CHANNEL_NORMALS | CHANNEL_UV0 | CHANNEL_COLORS;

/// A stored mesh's description, also the in/out record of a read. On a store, `generation`,
/// `present`, `changed` and `copied` are ignored. On a read, `generation` is the generation the
/// caller already holds and the three counts are the capacities of the buffers passed (all zero
/// asks for sizes only); the call fills in everything. `channels` is a mask of
/// [`CHANNEL_NORMALS`], [`CHANNEL_UV0`] and [`CHANNEL_COLORS`]; a channel outside it reads as
/// zero in every vertex. `left_out` counts the sub-meshes that are not triangle lists, which the
/// engine leaves out.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshInfo {
    pub identity: u64,
    /// Starts at 1 and rises by one each time the identity is stored again.
    pub generation: u64,
    /// Zero when a read names an identity nothing stored.
    pub present: u32,
    /// Nonzero when the stored generation is newer than the one the read passed.
    pub changed: u32,
    /// Nonzero when the read copied the buffers.
    pub copied: u32,
    pub channels: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub submesh_count: u32,
    pub left_out: u32,
}

/// One vertex: position, normal, first texture coordinate and colour (linear RGBA).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityMeshVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub nx: f32,
    pub ny: f32,
    pub nz: f32,
    pub u: f32,
    pub v: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A run of triangle indices that one material slot draws. `slot` is the sub-mesh's number in
/// the managed mesh, so a left-out sub-mesh leaves a gap in the slots.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshSubMesh {
    pub slot: u32,
    pub first_index: u32,
    pub index_count: u32,
}

/// One mesh of a world's session, kept until the world is unloaded.
pub(crate) struct StoredMesh {
    info: DisunityMeshInfo,
    vertices: Vec<DisunityMeshVertex>,
    indices: Vec<u32>,
    submeshes: Vec<DisunityMeshSubMesh>,
    /// Every channel and sub-mesh, stored after the fixed mesh by `disunity_mesh_channels_store`
    /// and dropped when the fixed mesh is stored again.
    pub(crate) channels: Option<StoredChannels>,
}

impl StoredMesh {
    pub(crate) fn generation(&self) -> u64 {
        self.info.generation
    }

    pub(crate) fn vertex_count(&self) -> u32 {
        self.info.vertex_count
    }
}

/// The meshes of one session, by renderer identity.
pub(crate) type MeshStore = BTreeMap<u64, StoredMesh>;

fn check_store(
    identity: u64,
    info: &DisunityMeshInfo,
    vertices: &[DisunityMeshVertex],
    indices: &[u32],
    submeshes: &[DisunityMeshSubMesh],
) -> Result<(), &'static str> {
    if identity < FIRST_PLAYER_MESH || info.identity != identity {
        return Err(
            "a stored mesh needs a player mesh identity of 2^41 or more, named in its description",
        );
    }
    if info.channels & !ALL_CHANNELS != 0 {
        return Err("a stored mesh names a channel the engine does not have");
    }
    if vertices.iter().any(|v| !finite(v)) {
        return Err("a stored mesh holds a vertex value that is not finite");
    }
    if !indices.len().is_multiple_of(3) || indices.iter().any(|i| *i as usize >= vertices.len()) {
        return Err("a stored mesh needs whole triangles over its own vertices");
    }
    for run in submeshes {
        let end = u64::from(run.first_index) + u64::from(run.index_count);
        if !run.first_index.is_multiple_of(3)
            || !run.index_count.is_multiple_of(3)
            || end > indices.len() as u64
        {
            return Err("a sub-mesh of a stored mesh must be whole triangles inside its indices");
        }
    }
    Ok(())
}

fn finite(v: &DisunityMeshVertex) -> bool {
    [
        v.x, v.y, v.z, v.nx, v.ny, v.nz, v.u, v.v, v.r, v.g, v.b, v.a,
    ]
    .iter()
    .all(|value| value.is_finite())
}

/// Reads `count` records from a pointer that is null only when `count` is zero.
///
/// # Safety
/// `pointer` is readable for `count` records.
pub(crate) unsafe fn records<'a, T>(pointer: *const T, count: u32) -> Option<&'a [T]> {
    if count == 0 {
        Some(&[])
    } else if pointer.is_null() {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(pointer, count as usize) })
    }
}

/// Keeps a mesh's contents for the host and raises its generation.
///
/// # Safety
/// `info` is readable for one record. `vertices`, `indices` and `submeshes` are readable for the
/// counts in the record, or null when that count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_store(
    world: u64,
    identity: u64,
    info: *const DisunityMeshInfo,
    vertices: *const DisunityMeshVertex,
    indices: *const u32,
    submeshes: *const DisunityMeshSubMesh,
) -> DisunityResult {
    if info.is_null() {
        return DisunityResult::invalid_argument();
    }
    let info = unsafe { *info };
    if info.vertex_count > MAX_VERTICES
        || info.index_count > MAX_INDICES
        || info.submesh_count > MAX_SUBMESHES
    {
        return DisunityResult::invalid_argument();
    }
    let (Some(vertices), Some(indices), Some(submeshes)) = (
        unsafe { records(vertices, info.vertex_count) },
        unsafe { records(indices, info.index_count) },
        unsafe { records(submeshes, info.submesh_count) },
    ) else {
        return DisunityResult::invalid_argument();
    };
    mutate(world, |s| {
        check_store(identity, &info, vertices, indices, submeshes)?;
        let generation = s
            .meshes
            .get(&identity)
            .map_or(1, |held| held.info.generation + 1);
        let info = DisunityMeshInfo {
            generation,
            present: 1,
            changed: 0,
            copied: 0,
            ..info
        };
        s.meshes.insert(
            identity,
            StoredMesh {
                info,
                vertices: vertices.to_vec(),
                indices: indices.to_vec(),
                submeshes: submeshes.to_vec(),
                channels: None,
            },
        );
        Ok(())
    })
}

/// Reports a stored mesh and copies its contents when they are newer than `info.generation` and
/// the capacities in `info` hold them. Capacities of zero everywhere copy nothing and report the
/// sizes. An identity nothing stored reports `present` zero and is not an error.
///
/// # Safety
/// `info` is readable and writable for one record; the buffers are writable for the capacities
/// in it, or null when that capacity is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_read(
    world: u64,
    identity: u64,
    info: *mut DisunityMeshInfo,
    vertices: *mut DisunityMeshVertex,
    indices: *mut u32,
    submeshes: *mut DisunityMeshSubMesh,
) -> DisunityResult {
    if info.is_null() {
        return DisunityResult::invalid_argument();
    }
    let request = unsafe { *info };
    let capacities = [
        request.vertex_count,
        request.index_count,
        request.submesh_count,
    ];
    let nulls = [vertices.is_null(), indices.is_null(), submeshes.is_null()];
    if capacities
        .iter()
        .zip(nulls)
        .any(|(c, null)| *c != 0 && null)
    {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let Some(held) = s.meshes.get(&identity) else {
            unsafe {
                *info = DisunityMeshInfo {
                    identity,
                    ..Default::default()
                };
            }
            return Ok(());
        };
        let changed = held.info.generation > request.generation;
        let copy = changed && capacities.iter().any(|c| *c != 0);
        if copy && !fits(held, capacities) {
            return Err("mesh copy capacity is too small".into());
        }
        unsafe {
            *info = DisunityMeshInfo {
                changed: u32::from(changed),
                copied: u32::from(copy),
                ..held.info
            };
            if copy {
                copy_out(held, vertices, indices, submeshes);
            }
        }
        Ok(())
    })
}

fn fits(held: &StoredMesh, [vertices, indices, submeshes]: [u32; 3]) -> bool {
    let info = held.info;
    vertices >= info.vertex_count && indices >= info.index_count && submeshes >= info.submesh_count
}

/// # Safety
/// Each pointer is writable for the stored mesh's matching count, or null when that count is zero.
unsafe fn copy_out(
    held: &StoredMesh,
    vertices: *mut DisunityMeshVertex,
    indices: *mut u32,
    submeshes: *mut DisunityMeshSubMesh,
) {
    unsafe {
        copy_records(&held.vertices, vertices);
        copy_records(&held.indices, indices);
        copy_records(&held.submeshes, submeshes);
    }
}

/// # Safety
/// `target` is writable for `source.len()` records, or null when that is zero.
pub(crate) unsafe fn copy_records<T: Copy>(source: &[T], target: *mut T) {
    if !source.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), target, source.len()) };
    }
}

#[cfg(test)]
#[path = "mesh_tests.rs"]
mod tests;
