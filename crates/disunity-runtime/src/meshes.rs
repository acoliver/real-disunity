//! Mesh contents and mesh renderers the managed engine gave the host, read from the same native
//! image.
use crate::{DisunityHost, HostError, native};

/// A run of triangle indices that one material slot draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshRun {
    /// The sub-mesh's number in the managed mesh; a left-out sub-mesh leaves a gap.
    pub slot: u32,
    pub first_index: u32,
    pub index_count: u32,
}

/// One mesh a player's managed code gave the renderer. The attribute vectors are empty for a
/// channel the mesh does not have; otherwise they hold one entry per position.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerMesh {
    /// Rises each time the managed engine stores the mesh again.
    pub generation: u64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    /// Triangle list indices over the positions.
    pub indices: Vec<u32>,
    pub submeshes: Vec<MeshRun>,
    /// How many sub-meshes are not triangle lists, which the engine leaves out.
    pub left_out: u32,
}

/// What one material slot of a mesh renderer draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshSlot {
    pub color: [f32; 4],
    /// The renderer identity of the slot's texture, 0 for none.
    pub texture: u64,
}

/// One mesh renderer the last prepared frame shows.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshObjectView {
    pub object: u64,
    /// The renderer identity of the mesh contents, the one [`DisunityHost::player_mesh`] takes.
    pub mesh: u64,
    /// Column-major matrix from the mesh's own coordinates to the frame's view basis.
    pub local_to_world: [f32; 16],
    pub slots: Vec<MeshSlot>,
}

type Read = unsafe extern "C" fn(
    u64,
    u64,
    *mut native::DisunityMeshInfo,
    *mut native::DisunityMeshVertex,
    *mut u32,
    *mut native::DisunityMeshSubMesh,
) -> native::DisunityResult;

type ObjectRead = unsafe extern "C" fn(
    u64,
    u32,
    *mut native::DisunityMeshObject,
    *mut native::DisunityMatrix,
    *mut native::DisunityDrawnMaterial,
    u32,
) -> native::DisunityResult;

impl DisunityHost {
    /// The mesh stored under renderer `identity`, or `None` when the managed engine stored no
    /// such mesh or its generation is still `known_generation`, so an unchanged mesh costs no
    /// copy.
    pub fn player_mesh(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerMesh>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        // The symbol belongs to the image this host retains.
        let read: Read = unsafe {
            *self
                ._library
                .get(b"disunity_mesh_read\0")
                .map_err(|_| HostError::MissingSymbol("disunity_mesh_read".into()))?
        };
        let mut info = native::DisunityMeshInfo {
            generation: known_generation,
            ..Default::default()
        };
        let sizes = unsafe {
            read(
                world,
                identity,
                &mut info,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        self.check(sizes)?;
        if info.present == 0 || info.changed == 0 {
            return Ok(None);
        }
        let mut vertices = vec![native::DisunityMeshVertex::default(); info.vertex_count as usize];
        let mut indices = vec![0u32; info.index_count as usize];
        let mut runs = vec![native::DisunityMeshSubMesh::default(); info.submesh_count as usize];
        let mut request = info;
        request.generation = known_generation;
        let copied = unsafe {
            read(
                world,
                identity,
                &mut request,
                vertices.as_mut_ptr(),
                indices.as_mut_ptr(),
                runs.as_mut_ptr(),
            )
        };
        self.check(copied)?;
        Ok(Some(split(&info, &vertices, indices, &runs)))
    }

    /// The mesh renderers the last call of `export_presentation` shows, in the world's order.
    pub fn mesh_objects(&self) -> Result<Vec<MeshObjectView>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let read: ObjectRead = unsafe {
            *self
                ._library
                .get(b"disunity_presentation_mesh_object\0")
                .map_err(|_| HostError::MissingSymbol("disunity_presentation_mesh_object".into()))?
        };
        let mut views = Vec::new();
        for index in 0..self.last_mesh_objects {
            views.push(self.mesh_object(world, read, index)?);
        }
        Ok(views)
    }

    fn mesh_object(
        &self,
        world: u64,
        read: ObjectRead,
        index: u32,
    ) -> Result<MeshObjectView, HostError> {
        let mut record = native::DisunityMeshObject::default();
        let mut matrix = native::DisunityMatrix::default();
        let null = std::ptr::null_mut();
        self.check(unsafe { read(world, index, &mut record, &mut matrix, null, 0) })?;
        let mut drawn = vec![native::DisunityDrawnMaterial::default(); record.slot_count as usize];
        if record.slot_count != 0 {
            let capacity = record.slot_count;
            self.check(unsafe {
                read(
                    world,
                    index,
                    &mut record,
                    &mut matrix,
                    drawn.as_mut_ptr(),
                    capacity,
                )
            })?;
        }
        Ok(MeshObjectView {
            object: record.object,
            mesh: record.mesh,
            local_to_world: matrix_array(&matrix),
            slots: drawn
                .iter()
                .map(|slot| MeshSlot {
                    color: [slot.r, slot.g, slot.b, slot.a],
                    texture: slot.texture,
                })
                .collect(),
        })
    }
}

fn split(
    info: &native::DisunityMeshInfo,
    vertices: &[native::DisunityMeshVertex],
    indices: Vec<u32>,
    runs: &[native::DisunityMeshSubMesh],
) -> PlayerMesh {
    let has = |channel: u32| info.channels & channel != 0;
    PlayerMesh {
        generation: info.generation,
        positions: vertices.iter().map(|v| [v.x, v.y, v.z]).collect(),
        normals: if has(1) {
            vertices.iter().map(|v| [v.nx, v.ny, v.nz]).collect()
        } else {
            Vec::new()
        },
        uvs: if has(2) {
            vertices.iter().map(|v| [v.u, v.v]).collect()
        } else {
            Vec::new()
        },
        colors: if has(4) {
            vertices.iter().map(|v| [v.r, v.g, v.b, v.a]).collect()
        } else {
            Vec::new()
        },
        indices,
        submeshes: runs
            .iter()
            .map(|run| MeshRun {
                slot: run.slot,
                first_index: run.first_index,
                index_count: run.index_count,
            })
            .collect(),
        left_out: info.left_out,
    }
}

fn matrix_array(m: &native::DisunityMatrix) -> [f32; 16] {
    [
        m.m0, m.m1, m.m2, m.m3, m.m4, m.m5, m.m6, m.m7, m.m8, m.m9, m.m10, m.m11, m.m12, m.m13,
        m.m14, m.m15,
    ]
}
