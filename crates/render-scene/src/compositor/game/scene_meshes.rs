//! The resident scene meshes a game's passes draw: the vertex and index buffers the stock
//! renderer uploaded for the scene, lent by scene mesh handle.
use super::lent::{FrameInputError, Lent};
use crate::draw::{LentLevel, LentMesh};
use std::ops::Range;
use std::rc::Rc;
use worldrunner_render_plan::{MaterialHandle, MeshHandle};
use worldrunner_render_program::runtime::{ImportedIndices, ImportedVertexInput};

/// Why a lent scene mesh cannot be bound.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameSceneMeshError {
    #[error("scene mesh {handle} is not in the resident table of {resident}")]
    Unknown { handle: u32, resident: usize },
    #[error("scene mesh {mesh} has {levels} levels, not {level}")]
    UnknownLevel {
        mesh: u32,
        level: usize,
        levels: usize,
    },
    #[error("level {level} of scene mesh {mesh} is skinned, and its palette is not lent")]
    Skinned { mesh: u32, level: usize },
    #[error("indices {range:?} are outside the {count} of level {level} of scene mesh {mesh}")]
    IndexRange {
        mesh: u32,
        level: usize,
        range: Range<u32>,
        count: u32,
    },
}

/// The part of a level's indices that one material draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneMeshSlot {
    pub material: MaterialHandle,
    pub indices: Range<u32>,
}

/// The resident meshes of the scene the frame in flight draws, indexed by scene mesh handle:
/// every mesh of the scene's table, uploaded once, including those appended since. One snapshot
/// is shared by every frame until the table changes.
///
/// A mesh is a chain of levels of detail, finest first, and each level has a vertex buffer and
/// an index buffer of its own. The vertices are in the stock layout
/// ([`geometry::vertex_attributes`](crate::geometry::vertex_attributes), stride
/// [`geometry::VERTEX_STRIDE`](crate::geometry::VERTEX_STRIDE)): position in metres at location
/// 0 (in the mesh's own space), normal at 1, texture coordinate at 2, billboard mode at 8,
/// baked occlusion at 7 and tangent with handedness at 11. Indices are `u32` triangle lists with
/// a base vertex of zero. A level carries no placement: a game supplies its own transforms.
#[derive(Clone, Debug)]
pub struct FrameSceneMeshes(Rc<[LentMesh]>);

impl FrameSceneMeshes {
    pub(crate) fn new(meshes: Rc<[LentMesh]>) -> Self {
        Self(meshes)
    }

    /// Meshes in the resident table.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn mesh(&self, mesh: MeshHandle) -> Result<&LentMesh, FrameSceneMeshError> {
        usize::try_from(mesh.raw())
            .ok()
            .and_then(|index| self.0.get(index))
            .ok_or(FrameSceneMeshError::Unknown {
                handle: mesh.raw(),
                resident: self.0.len(),
            })
    }

    fn level(&self, mesh: MeshHandle, level: usize) -> Result<&LentLevel, FrameSceneMeshError> {
        let lent = self.mesh(mesh)?;
        lent.levels
            .get(level)
            .ok_or(FrameSceneMeshError::UnknownLevel {
                mesh: mesh.raw(),
                level,
                levels: lent.levels.len(),
            })
    }

    /// Levels of detail of `mesh`.
    ///
    /// # Errors
    /// [`FrameSceneMeshError::Unknown`] for a handle beyond the resident table.
    pub fn levels(&self, mesh: MeshHandle) -> Result<usize, FrameSceneMeshError> {
        Ok(self.mesh(mesh)?.levels.len())
    }

    /// Indices of level `level` of `mesh`.
    ///
    /// # Errors
    /// [`FrameSceneMeshError::Unknown`] and [`FrameSceneMeshError::UnknownLevel`].
    pub fn index_count(&self, mesh: MeshHandle, level: usize) -> Result<u32, FrameSceneMeshError> {
        Ok(self.level(mesh, level)?.index_count)
    }

    /// The materials the scene authored for the indices of a level, as a partition of them in
    /// order. Empty when the level draws with the material of its placement.
    ///
    /// # Errors
    /// [`FrameSceneMeshError::Unknown`] and [`FrameSceneMeshError::UnknownLevel`].
    pub fn material_slots(
        &self,
        mesh: MeshHandle,
        level: usize,
    ) -> Result<Vec<SceneMeshSlot>, FrameSceneMeshError> {
        Ok(self
            .level(mesh, level)?
            .slots
            .iter()
            .map(|(material, indices)| SceneMeshSlot {
                material: *material,
                indices: indices.clone(),
            })
            .collect())
    }

    /// The vertex buffer a level's vertices are in, in the stock vertex layout. Other levels share
    /// it; [`Self::indices`] gives the base vertex that reaches this level's vertices.
    ///
    /// # Errors
    /// [`FrameSceneMeshError::Unknown`], [`FrameSceneMeshError::UnknownLevel`] and
    /// [`FrameSceneMeshError::Skinned`] for a level whose vertices carry skin influences, which
    /// need the scene's skin palette that is not lent.
    pub fn vertex_input(
        &self,
        mesh: MeshHandle,
        level: usize,
    ) -> Result<ImportedVertexInput, FrameSceneMeshError> {
        let lent = self.level(mesh, level)?;
        if lent.skinned {
            return Err(FrameSceneMeshError::Skinned {
                mesh: mesh.raw(),
                level,
            });
        }
        Ok(ImportedVertexInput {
            buffer: lent.vertices.clone(),
            stride: crate::geometry::VERTEX_STRIDE,
            step: wgpu::VertexStepMode::Vertex,
            attributes: crate::geometry::vertex_attributes().to_vec(),
        })
    }

    /// The indices `range` of a level, as `u32`, offset into the shared index buffer the level is
    /// in and with the base vertex of its vertices. The whole level is `0..index_count`.
    ///
    /// # Errors
    /// [`FrameSceneMeshError::Unknown`], [`FrameSceneMeshError::UnknownLevel`] and
    /// [`FrameSceneMeshError::IndexRange`] for a range that is not within the level.
    pub fn indices(
        &self,
        mesh: MeshHandle,
        level: usize,
        range: Range<u32>,
    ) -> Result<ImportedIndices, FrameSceneMeshError> {
        let lent = self.level(mesh, level)?;
        if range.start > range.end || range.end > lent.index_count {
            return Err(FrameSceneMeshError::IndexRange {
                mesh: mesh.raw(),
                level,
                range,
                count: lent.index_count,
            });
        }
        Ok(ImportedIndices {
            buffer: lent.indices.clone(),
            format: wgpu::IndexFormat::Uint32,
            first: lent.first_index + range.start,
            count: range.end - range.start,
            base_vertex: lent.base_vertex,
        })
    }

    /// Whether `other` is the same snapshot: the same table, so anything built from one draws
    /// the buffers the other would. An append or a mesh replacement makes a new snapshot.
    pub fn same_snapshot(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where a game reads the resident scene meshes of the frame in flight.
#[derive(Clone, Debug, Default)]
pub struct FrameSceneMeshesHandle(Lent<FrameSceneMeshes>);

impl FrameSceneMeshesHandle {
    /// # Errors
    /// [`FrameInputError::OutsideFrame`] outside a frame and [`FrameInputError::NotPrepared`]
    /// before the route has published them.
    pub fn current(&self) -> Result<FrameSceneMeshes, FrameInputError> {
        self.0.current()
    }

    pub(crate) fn publish(&self, meshes: impl FnOnce() -> FrameSceneMeshes) {
        self.0.publish_with(meshes);
    }
}

impl From<FrameSceneMeshError> for worldrunner_render_post::FrameError {
    fn from(error: FrameSceneMeshError) -> Self {
        Self::FeaturePreparation {
            feature: "frame scene meshes",
            reason: error.to_string(),
        }
    }
}
