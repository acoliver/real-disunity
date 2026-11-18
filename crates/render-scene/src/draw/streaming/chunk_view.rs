use super::{SceneGpu, Streaming};
use crate::{frame::FrameUniform, terrain::streaming::TerrainChunk};
use std::collections::BTreeMap;
use worldrunner_render_gpu::submission::{Publications, TransactionStatus};

/// A boundary of a chunk in the heightfield sample grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainChunkEdge {
    West,
    East,
    South,
    North,
}

/// A geometrically adjacent leaf and the segment it shares with the draw chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainChunkNeighbor {
    /// The adjacent leaf, including its geometric sampling stride.
    pub chunk: TerrainChunk,
    /// Inclusive endpoints on the axis along the edge, in height samples.
    /// The first endpoint is strictly smaller than the second.
    pub overlap: [u32; 2],
}

/// The resident GPU mesh of a chunk, as the stock camera draw binds it.
///
/// Vertices use the stock layout, [`crate::geometry::VERTEX_STRIDE`] bytes each and described by
/// [`crate::geometry::vertex_attributes`], with positions in scene metres. Indices are `u32`
/// triangle lists including the skirts. The buffers are the renderer's own: read them in the
/// frame that lent them and do not write them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainChunkBuffers {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
}

/// Geometry metadata and the resident mesh of one chunk submitted by a streamed camera view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainChunkDraw {
    pub chunk: TerrainChunk,
    neighbors: [Vec<TerrainChunkNeighbor>; 4],
    buffers: TerrainChunkBuffers,
}

impl TerrainChunkDraw {
    /// The chunk's resident vertex and index buffers.
    pub fn buffers(&self) -> &TerrainChunkBuffers {
        &self.buffers
    }

    /// Adjacent residents sorted by the start of their overlap along this edge.
    /// A chunk outside the raster frustum can still border this one. Corner-only
    /// contacts are excluded. An outer edge has no neighbors.
    pub fn neighbors(&self, edge: TerrainChunkEdge) -> &[TerrainChunkNeighbor] {
        &self.neighbors[edge as usize]
    }
}

/// A streamed terrain view cannot be read from the active frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TerrainChunkViewError {
    #[error("streaming terrain has no prepared selection for this scene and frame")]
    Unprepared,
    #[error("streaming terrain was prepared for a different selection projection or viewport")]
    ViewMismatch,
    #[error("an earlier capture segment replaced the prepared streaming terrain cover")]
    Replaced,
}

impl SceneGpu {
    /// Reads the chunks the stock camera draw would submit, in draw order.
    ///
    /// Call inside a live frame transaction after [`Self::prepare_terrain_geometry`]
    /// or stock scene preparation. Selection and viewport must match the prepared
    /// view. The supplied raster projection (including jitter) and camera position
    /// govern culling and order, as in a stock draw. Neighbor strides describe
    /// geometry; the caller owns any material-node or index-map coordinates.
    /// `None` means the scene does not stream terrain. The metadata is for this frame.
    ///
    /// # Errors
    /// Returns [`TerrainChunkViewError`] if the scene has no preparation in this
    /// frame or the requested selection projection or viewport differs.
    pub fn prepared_terrain_chunks(
        &self,
        frame: &FrameUniform,
        viewport: [u32; 2],
    ) -> Result<Option<Vec<TerrainChunkDraw>>, TerrainChunkViewError> {
        self.terrain
            .as_ref()
            .and_then(|terrain| terrain.streaming.as_ref())
            .map(|streaming| streaming.chunk_view(frame, viewport))
            .transpose()
    }
}

impl Streaming {
    fn chunk_view(
        &self,
        frame: &FrameUniform,
        viewport: [u32; 2],
    ) -> Result<Vec<TerrainChunkDraw>, TerrainChunkViewError> {
        let owner = Publications::current()
            .filter(|owner| owner.status() == TransactionStatus::Preparing)
            .ok_or(TerrainChunkViewError::Unprepared)?;
        {
            let drawing = self.drawing.borrow();
            let prepared = drawing
                .as_ref()
                .filter(|prepared| owner.same_frame(&prepared.owner))
                .ok_or(TerrainChunkViewError::Unprepared)?;
            if !prepared.matches_view(frame, viewport) {
                return Err(TerrainChunkViewError::ViewMismatch);
            }
            // A later segment reads the published cover, which an earlier segment's other
            // publications (a retained replacement coarsening this source) may have replaced.
            if !owner.same_transaction(&prepared.owner)
                && !same_cover(&prepared.residents, &self.committed.borrow())
            {
                return Err(TerrainChunkViewError::Replaced);
            }
        }
        Ok(self.with_residents(Some(&owner), |residents, _| {
            let topology = Adjacency::new(residents.iter().map(|&(chunk, _)| chunk));
            self.ordered_where(
                residents,
                Some(frame.camera_position),
                self.camera_keep(frame),
            )
            .into_iter()
            .map(|(chunk, lod)| TerrainChunkDraw {
                chunk: *chunk,
                neighbors: std::array::from_fn(|i| topology.edge(*chunk, i / 2, i % 2)),
                buffers: TerrainChunkBuffers {
                    vertices: lod.vertices.clone(),
                    indices: lod.indices.clone(),
                    index_count: lod.index_count,
                },
            })
            .collect()
        }))
    }
}

fn same_cover(prepared: &super::Residents, committed: &super::Residents) -> bool {
    prepared.len() == committed.len()
        && prepared
            .iter()
            .all(|(chunk, _)| committed.iter().any(|(other, _)| other == chunk))
}

#[derive(Default)]
struct Boundary {
    sides: [Vec<TerrainChunk>; 2],
}

/// Each side of a fixed boundary has disjoint, ordered intervals in admitted leaves.
struct Adjacency {
    boundaries: [BTreeMap<u32, Boundary>; 2],
}

impl Adjacency {
    fn new(chunks: impl IntoIterator<Item = TerrainChunk>) -> Self {
        let mut boundaries = [BTreeMap::<u32, Boundary>::new(), BTreeMap::new()];
        for chunk in chunks {
            for (axis, index) in boundaries.iter_mut().enumerate() {
                for (side, coordinate) in [chunk.minimum[axis], chunk.maximum[axis]]
                    .into_iter()
                    .enumerate()
                {
                    index.entry(coordinate).or_default().sides[side].push(chunk);
                }
            }
        }
        for (axis, index) in boundaries.iter_mut().enumerate() {
            for boundary in index.values_mut() {
                for chunks in &mut boundary.sides {
                    chunks.sort_unstable_by_key(|chunk| chunk.minimum[1 - axis]);
                }
            }
        }
        Self { boundaries }
    }

    fn edge(&self, chunk: TerrainChunk, axis: usize, side: usize) -> Vec<TerrainChunkNeighbor> {
        let coordinate = [chunk.minimum[axis], chunk.maximum[axis]][side];
        let Some(boundary) = self.boundaries[axis].get(&coordinate) else {
            return Vec::new();
        };
        let along = 1 - axis;
        let [low, high] = [chunk.minimum[along], chunk.maximum[along]];
        let others = &boundary.sides[1 - side];
        let first = others.partition_point(|other| other.maximum[along] <= low);
        others[first..]
            .iter()
            .take_while(|other| other.minimum[along] < high)
            .map(|&other| TerrainChunkNeighbor {
                chunk: other,
                overlap: [
                    other.minimum[along].max(low),
                    other.maximum[along].min(high),
                ],
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
