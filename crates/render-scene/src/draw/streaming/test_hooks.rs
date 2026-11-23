//! Test-only accessors of this module's types.

use super::*;

impl Streaming {
    /// The chunks a stock draw recorded now would submit, before culling.
    pub(crate) fn drawn_chunks(&self) -> Vec<TerrainChunk> {
        self.with_residents(Publications::current().as_ref(), |residents, _| {
            residents.iter().map(|(chunk, _)| *chunk).collect()
        })
    }

    pub(crate) fn prepared_buffers(&self) -> Vec<wgpu::Buffer> {
        self.drawing
            .borrow()
            .as_ref()
            .unwrap()
            .residents
            .iter()
            .map(|(_, lod)| lod.vertices.clone())
            .collect()
    }

    /// The vertex buffers of the chunks that are committed or being drawn.
    pub(crate) fn resident_buffers(&self) -> Vec<wgpu::Buffer> {
        let drawing = self.drawing.borrow();
        let drawn = drawing.iter().flat_map(|prepared| &prepared.residents);
        self.committed
            .borrow()
            .iter()
            .chain(drawn)
            .map(|(_, lod)| lod.vertices.clone())
            .collect()
    }

    pub(crate) fn allocation_counts(&self) -> (usize, usize, usize) {
        let pending = self
            .drawing
            .borrow()
            .as_ref()
            .filter(|prepared| {
                prepared.owner.status()
                    != worldrunner_render_gpu::submission::TransactionStatus::Submitted
            })
            .map_or(0, |prepared| prepared.uploads);
        (
            self.committed.borrow().len(),
            pending,
            self.retired
                .borrow()
                .iter()
                .map(|(_, chunks)| chunks.len())
                .sum(),
        )
    }
}

impl SceneGpu {
    /// The vertex buffers the program geometry cache holds for streamed chunks.
    pub(crate) fn cached_chunk_buffers(&self) -> Vec<wgpu::Buffer> {
        self.terrain
            .as_ref()
            .and_then(|terrain| terrain.streaming.as_ref())
            .map_or_else(Vec::new, |streaming| streaming.geometry.buffers())
    }
}
