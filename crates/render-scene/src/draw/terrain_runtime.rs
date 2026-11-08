use super::{DrawCount, GpuTerrain, SceneGpu, TERRAIN_GROUP};
use crate::renderer::ScenePipelines;
use wgpu::{BindGroup, IndexFormat, RenderPass};

impl SceneGpu {
    /// Records the terrain through the program runtime: the prepared pipeline and material set
    /// once, the shadow group when there is one, then the geometry. The frame group and the
    /// screen group are the pass's, bound before this.
    ///
    /// The pipeline, the set and the geometry are prepared the first time a pass needs them and
    /// found in maps afterwards, so a steady-state frame builds nothing.
    ///
    /// # Panics
    /// Panics when the runtime refuses the terrain's program, which the program tests rule out
    /// for every context the crate builds.
    pub(crate) fn draw_terrain_runtime<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) -> (DrawCount, bool) {
        use crate::terrain::program::Surface;
        let Some(terrain) = &self.terrain else {
            return (DrawCount::default(), false);
        };
        let slot = match terrain
            .pages
            .as_ref()
            .map(|pages| pages.borrow().draw_slot())
        {
            Some(None) => return (DrawCount::default(), false),
            Some(Some(slot)) => Some(slot),
            None => None,
        };
        let surface = Surface::new(terrain.layered.is_some(), slot.is_some());
        let pages = || terrain_pages(terrain, slot);
        let pipeline = terrain
            .prepared
            .pipeline(&crate::terrain::prepared::Request {
                context: pipelines.terrain_context(),
                authority: &terrain.authority,
                resources: &terrain.resources,
                surface,
                slot: slot.unwrap_or(0),
                pages: &pages,
            })
            .expect("the terrain's program prepares against the layouts of its context");
        pipeline.bind(pass);
        if let Some(shadow) = shadow {
            pass.set_bind_group(2, shadow);
        }
        let count = if let Some(streaming) = &terrain.streaming {
            streaming.draw_prepared(pass, &pipeline, terrain)
        } else {
            let lod = terrain.lod.as_ref().expect("nonstreaming mesh");
            terrain.prepared.draw_mesh(
                pass,
                &pipeline,
                (&lod.vertices, &lod.indices, lod.index_count),
                &terrain.instances,
            );
            DrawCount {
                calls: 1,
                instances: 1,
            }
        };
        #[cfg(test)]
        paths::runtime();
        (count, true)
    }

    /// Records the terrain through the fixed pipelines, for the passes that have not moved to
    /// the program runtime.
    pub(super) fn draw_terrain_fixed(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) -> (DrawCount, bool) {
        let mut count = DrawCount::default();
        let mut bound = false;
        if let Some(terrain) = &self.terrain {
            let cached_group = terrain
                .pages
                .as_ref()
                .map(|pages| pages.borrow().draw_group());
            if !matches!(cached_group, Some(None)) {
                if let Some(Some(group)) = cached_group {
                    pass.set_pipeline(if terrain.layered.is_some() {
                        pipelines
                            .layered
                            .as_ref()
                            .expect("validated layered device")
                            .cached()
                    } else {
                        pipelines.cached_terrain()
                    });
                    pass.set_bind_group(TERRAIN_GROUP, &group, &[]);
                } else {
                    pass.set_pipeline(if terrain.layered.is_some() {
                        pipelines
                            .layered
                            .as_ref()
                            .expect("validated layered device")
                            .terrain()
                    } else {
                        pipelines.terrain()
                    });
                    pass.set_bind_group(TERRAIN_GROUP, &terrain.bind_group, &[]);
                }
                if let Some(shadow) = shadow {
                    pass.set_bind_group(2, shadow, &[]);
                }
                bound = true;
                #[cfg(test)]
                paths::fixed();
                count = Self::draw_terrain_geometry(pass, terrain);
            }
        }
        (count, bound)
    }

    pub(super) fn draw_terrain_geometry(
        pass: &mut RenderPass<'_>,
        terrain: &GpuTerrain,
    ) -> DrawCount {
        if let Some(streaming) = &terrain.streaming {
            return streaming.draw(pass, &terrain.instances);
        }
        let lod = terrain.lod.as_ref().expect("nonstreaming mesh");
        pass.set_vertex_buffer(0, lod.vertices.slice(..));
        pass.set_vertex_buffer(1, terrain.instances.slice(..));
        pass.set_index_buffer(lod.indices.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..lod.index_count, 0, 0..1);
        DrawCount {
            calls: 1,
            instances: 1,
        }
    }
}

/// The page cache and the page table of `slot` that a pages surface's material set reads.
fn terrain_pages(terrain: &GpuTerrain, slot: Option<usize>) -> crate::terrain::prepared::Pages {
    let pages = terrain
        .pages
        .as_ref()
        .expect("a pages surface has pages")
        .borrow();
    crate::terrain::prepared::Pages {
        texture: pages.cache.texture(),
        view: pages.cache.texture_view(),
        table: pages.tables[slot.expect("a pages surface has a slot")].clone(),
    }
}

/// How many times this thread recorded terrain through each path, for tests that prove a draw
/// took the path it names. A terrain that draws nothing counts nowhere.
#[cfg(test)]
pub(crate) mod paths {
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Recorded {
        pub runtime: u64,
        pub fixed: u64,
    }

    thread_local! {
        static RECORDED: Cell<Recorded> = const { Cell::new(Recorded { runtime: 0, fixed: 0 }) };
    }

    pub(crate) fn recorded() -> Recorded {
        RECORDED.get()
    }

    pub(super) fn runtime() {
        let now = RECORDED.get();
        RECORDED.set(Recorded {
            runtime: now.runtime + 1,
            ..now
        });
    }

    pub(super) fn fixed() {
        let now = RECORDED.get();
        RECORDED.set(Recorded {
            fixed: now.fixed + 1,
            ..now
        });
    }
}
