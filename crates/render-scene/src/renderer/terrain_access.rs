//! How a [`ScenePipelines`] reaches its terrain: the programs of its context on the runtime.

use super::ScenePipelines;

impl ScenePipelines {
    /// What the program runtime that records terrain has built.
    #[cfg(test)]
    pub(crate) fn terrain_runtime_stats(
        &self,
    ) -> worldrunner_render_program::runtime::RuntimeStats {
        self.terrain_context.family().stats()
    }

    /// Whether the device these pipelines were built for holds the layered terrain sources.
    pub(crate) fn layered_terrain_supported(&self) -> bool {
        crate::terrain::pages::layered_gpu::supported(&self.terrain_context.device().limits())
    }

    /// The terrain programs of this context.
    pub(crate) fn terrain_context(&self) -> &crate::terrain::program::Context {
        &self.terrain_context
    }
}
