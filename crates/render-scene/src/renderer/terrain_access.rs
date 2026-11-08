//! How a [`ScenePipelines`] reaches its terrain: the programs on the runtime and the fixed
//! pipelines it keeps for contexts whose screen group the program does not lend.

use wgpu::{BindGroupLayout, RenderPipeline};

use super::ScenePipelines;

impl ScenePipelines {
    /// What the program runtime that records terrain has built.
    #[cfg(test)]
    pub(crate) fn terrain_runtime_stats(
        &self,
    ) -> worldrunner_render_program::runtime::RuntimeStats {
        self.terrain_context.family().stats()
    }

    /// Returns the fixed pipeline the terrain surface draws through, building it on first use.
    #[must_use]
    pub fn terrain(&self) -> &RenderPipeline {
        self.terrain.terrain()
    }

    /// The fixed pipeline that reads cached material pages, built on first use.
    pub(crate) fn cached_terrain(&self) -> &RenderPipeline {
        self.terrain.cached()
    }

    /// The layout of the fixed cached-page group.
    pub(crate) fn cached_terrain_layout(&self) -> &BindGroupLayout {
        self.terrain.cached_layout()
    }

    /// Whether any fixed terrain pipeline of these pipelines has been compiled.
    #[cfg(test)]
    pub(crate) fn fixed_terrain_built(&self) -> bool {
        self.terrain.is_built()
            || self
                .layered
                .as_ref()
                .is_some_and(super::pages::Pair::is_built)
    }

    /// The terrain programs of this context.
    pub(crate) fn terrain_context(&self) -> &crate::terrain::program::Context {
        &self.terrain_context
    }

    /// Whether terrain draws through the program runtime in this context.
    pub(crate) fn terrain_on_runtime(&self) -> bool {
        self.terrain_context.on_runtime()
    }
}
