use super::{DrawCount, GpuTerrain, SceneGpu, TERRAIN_GROUP};
use crate::renderer::ScenePipelines;
use crate::terrain::pages::scene::ScenePages;
use wgpu::{BindGroup, IndexFormat, RenderPass};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::PreparedPipeline;

impl SceneGpu {
    /// The prepared pipeline the terrain draws with in `pipelines`, made the first time a pass
    /// needs it and found in a map afterwards. `None` when the scene has no terrain or its pages
    /// have nothing to draw this frame.
    fn terrain_pipeline(
        &self,
        pipelines: &ScenePipelines,
        page_slot: impl Fn(&ScenePages) -> Option<usize>,
    ) -> Result<Option<std::rc::Rc<PreparedPipeline>>, ProgramError> {
        use crate::terrain::program::Surface;
        let Some(terrain) = &self.terrain else {
            return Ok(None);
        };
        let slot = match terrain
            .pages
            .as_ref()
            .map(|pages| page_slot(&pages.borrow()))
        {
            Some(None) => return Ok(None),
            Some(Some(slot)) => Some(slot),
            None => None,
        };
        let surface = Surface::new(terrain.layered.is_some(), slot.is_some());
        let pages = || terrain_pages(terrain, slot);
        terrain
            .prepared
            .pipeline(&crate::terrain::prepared::Request {
                context: pipelines.terrain_context(),
                authority: &terrain.authority,
                resources: &terrain.resources,
                surface,
                slot: slot.unwrap_or(0),
                pages: &pages,
            })
            .map(Some)
    }

    /// Prepares the terrain's pipeline for a pass drawn with `pipelines` in the frame `frame`,
    /// before the pass opens, where a refusal is still an error rather than a lost draw. The
    /// pipelines are selected for the frame's receiver, convention and winding as the draw
    /// selects them. A pass that was not prepared makes its pipeline when it first draws.
    ///
    /// # Errors
    /// Fails when the runtime refuses the terrain's program, set or pipeline for the context.
    pub(crate) fn prepare_terrain(
        &self,
        pipelines: &ScenePipelines,
        frame: &crate::frame::FrameUniform,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        let selected = self.selected_pipelines(pipelines, frame);
        if self.terrain.is_none() || !selected.terrain_on_runtime() {
            return Ok(());
        }
        self.terrain_pipeline(&selected, ScenePages::prepared_slot)
            .map(drop)
            .map_err(
                |error| worldrunner_render_post::FrameError::FeaturePreparation {
                    feature: "terrain",
                    reason: error.to_string(),
                },
            )
    }

    /// Prepares the terrain for a replay of the frame being drawn, as
    /// [`prepare_terrain`](Self::prepare_terrain) does for a frame about to be committed.
    ///
    /// # Errors
    /// As [`prepare_terrain`](Self::prepare_terrain).
    pub(crate) fn prepare_terrain_replay(
        &self,
        pipelines: &ScenePipelines,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        if self.terrain.is_none() {
            return Ok(());
        }
        self.prepare_terrain(pipelines, &self.drawing_uniform())
    }

    /// How many prepared pipelines and sets the terrain keeps.
    #[cfg(test)]
    pub(crate) fn kept_terrain(&self) -> (usize, usize) {
        self.terrain
            .as_ref()
            .map_or((0, 0), |terrain| terrain.prepared.kept())
    }

    /// The surface source the scene's terrain is drawn with.
    pub(crate) fn terrain_surface(&self) -> Option<crate::terrain::program::Surface> {
        self.terrain.as_ref().map(|terrain| {
            crate::terrain::program::Surface::new(
                terrain.layered.is_some(),
                terrain.pages.is_some(),
            )
        })
    }

    /// Records the terrain through the program runtime: the prepared pipeline and material set
    /// once, the shadow group when there is one, then the geometry. The frame group and the
    /// screen group are the pass's, bound before this.
    ///
    /// The pipeline, the set and the geometry are made the first time a pass needs them and
    /// found in maps afterwards, so a steady-state frame builds nothing. Frames that call
    /// [`prepare_terrain`](Self::prepare_terrain) before they open the pass find them made.
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
        let Some(terrain) = &self.terrain else {
            return (DrawCount::default(), false);
        };
        let Some(pipeline) = self
            .terrain_pipeline(pipelines, ScenePages::draw_slot)
            .expect("the terrain's program prepares against the layouts of its context")
        else {
            return (DrawCount::default(), false);
        };
        pipeline.bind(pass);
        if let Some(shadow) = shadow {
            pass.set_bind_group(2, shadow);
        }
        let count = if let Some(streaming) = &terrain.streaming {
            streaming.draw_prepared(pass, &pipeline, terrain)
        } else {
            Self::draw_terrain_mesh(pass, &pipeline, terrain)
        };
        #[cfg(test)]
        paths::runtime();
        (count, true)
    }

    /// Records the one mesh of a terrain that is not streamed through a prepared pipeline.
    pub(super) fn draw_terrain_mesh<P: worldrunner_render_program::runtime::PassCommands>(
        pass: &mut P,
        pipeline: &PreparedPipeline,
        terrain: &GpuTerrain,
    ) -> DrawCount {
        let lod = terrain.lod.as_ref().expect("nonstreaming mesh");
        terrain.prepared.draw_mesh(
            pass,
            pipeline,
            (&lod.vertices, &lod.indices, lod.index_count),
            &terrain.instances,
        );
        DrawCount {
            calls: 1,
            instances: 1,
        }
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
