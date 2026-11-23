//! Test-only accessors of this module's types.

use super::*;

impl PreparedTerrain {
    /// How many pipelines and sets are kept, for the tests that hold the caches bounded.
    pub(crate) fn kept(&self) -> (usize, usize) {
        (self.pipelines.borrow().len(), self.sets.borrow().len())
    }
}

impl ChunkGeometry {
    /// The vertex buffers the chunk geometry holds.
    pub(crate) fn buffers(&self) -> Vec<Buffer> {
        self.chunks
            .borrow()
            .values()
            .map(|(vertices, _)| vertices.clone())
            .collect()
    }
}
