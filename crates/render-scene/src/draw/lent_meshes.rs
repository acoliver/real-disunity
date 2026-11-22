//! The resident scene meshes as a game's passes are lent them.
use super::SceneGpu;
use std::ops::Range;
use std::rc::Rc;
use wgpu::Buffer;
use worldrunner_render_plan::MaterialHandle;

/// One level of a resident mesh: the shared vertex and index chunks it is packed into, where in
/// them it starts, and the material partition of its indices, when the level has one.
#[derive(Clone, Debug)]
pub(crate) struct LentLevel {
    pub(crate) skinned: bool,
    pub(crate) vertices: Buffer,
    /// The vertex offset a draw adds to the level's indices: its first vertex in the chunk.
    pub(crate) base_vertex: i32,
    pub(crate) indices: Buffer,
    /// The level's first index in the index chunk.
    pub(crate) first_index: u32,
    pub(crate) index_count: u32,
    pub(crate) slots: Vec<(MaterialHandle, Range<u32>)>,
}

/// The levels of one resident mesh, finest first.
#[derive(Clone, Debug)]
pub(crate) struct LentMesh {
    pub(crate) levels: Vec<LentLevel>,
}

impl SceneGpu {
    /// The scene's terrain, shared with the scene that was uploaded, so its identity changes
    /// only when the terrain does.
    pub(crate) fn lent_terrain(
        &self,
    ) -> Option<std::sync::Arc<worldrunner_render_plan::SceneTerrain>> {
        self.terrain
            .as_ref()
            .map(|terrain| terrain.resources.scene.clone())
    }

    /// Every resident scene mesh by table index, as one shared snapshot that stays the same
    /// object until an append or a mesh replacement changes the table.
    pub(crate) fn lent_meshes(&self) -> Rc<[LentMesh]> {
        self.retained
            .lent_meshes
            .borrow_mut()
            .get_or_insert_with(|| {
                self.meshes
                    .iter()
                    .zip(&self.mesh_selection)
                    .map(|(mesh, selection)| LentMesh {
                        levels: mesh
                            .lods
                            .iter()
                            .zip(&selection.material_slots)
                            .map(|(lod, slots)| LentLevel {
                                skinned: lod.layout.is_skinned(),
                                vertices: lod.vertices.buffer().clone(),
                                base_vertex: lod.vertices.base_vertex(),
                                indices: lod.indices.buffer().clone(),
                                first_index: lod.indices.first(),
                                index_count: lod.index_count,
                                slots: slots
                                    .iter()
                                    .flatten()
                                    .map(|slot| (slot.material(), slot.indices()))
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect()
            })
            .clone()
    }
}
