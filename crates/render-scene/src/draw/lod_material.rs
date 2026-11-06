use super::{MATERIAL_GROUP, SceneGpu, visibility::SelectedRange};

/// Render state the current selection's draws have already set. wgpu validates and encodes
/// every state call again, including a pipeline change to the pipeline already bound, so
/// repeats are skipped. A pipeline change can invalidate the bound material group, so a
/// material is only reused under the pipeline it was bound with.
#[derive(Debug, Default)]
pub(super) struct BoundState {
    pipeline: Option<usize>,
    material: Option<usize>,
}

impl BoundState {
    /// Records that `pipeline` and `material` are bound and returns which of them must be set.
    fn bind(&mut self, pipeline: usize, material: usize) -> (bool, bool) {
        let set_pipeline = self.pipeline != Some(pipeline);
        if set_pipeline {
            self.pipeline = Some(pipeline);
            self.material = None;
        }
        let set_material = self.material != Some(material);
        self.material = Some(material);
        (set_pipeline, set_material)
    }
}

impl SceneGpu {
    pub(super) fn record_lod_range(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        range: &SelectedRange,
        state: &mut BoundState,
    ) -> super::DrawCount {
        let batch = &self.instances.batches[&range.key];
        pass.set_vertex_buffer(
            1,
            batch
                .buffer
                .as_ref()
                .expect("flushed instance buffer")
                .slice(..),
        );
        // Only velocity pipelines declare this buffer; the others ignore the extra binding.
        if let Some(previous) = &batch.previous {
            pass.set_vertex_buffer(2, previous.slice(..));
        }
        self.record_lod_draws(pass, pipelines, range, state)
    }

    /// Draws the instances `range.slots` of the buffer already bound as vertex buffer 1.
    pub(super) fn record_lod_draws(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        range: &SelectedRange,
        state: &mut BoundState,
    ) -> super::DrawCount {
        let (_, mesh, _) = range.key;
        let lod = &self.meshes[mesh].lods[range.lod];
        pass.set_vertex_buffer(0, lod.vertices.slice(..));
        let mut count = self.record_lod_slots(pass, (pipelines, state), range, range.lod);
        if let Some(vertices) = &lod.transition {
            pass.set_vertex_buffer(0, vertices.slice(..));
            // The complement belongs to the preceding level, including its materials.
            count.add(self.record_lod_slots(pass, (pipelines, state), range, range.lod - 1));
        }
        count
    }

    fn record_lod_slots(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        bind: (
            &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
            &mut BoundState,
        ),
        range: &SelectedRange,
        level: usize,
    ) -> super::DrawCount {
        let (pipelines, state) = bind;
        let mut count = super::DrawCount::default();
        let instances = u64::from(range.slots.end - range.slots.start);
        let (pipeline, mesh, material) = range.key;
        let lod = &self.meshes[mesh].lods[level];
        let mut draw = |pass: &mut wgpu::RenderPass<'_>, pipeline: usize, material: usize| {
            let (set_pipeline, set_material) = state.bind(pipeline, material);
            if set_pipeline {
                pass.set_pipeline(&pipelines[pipeline]);
            }
            if set_material {
                pass.set_bind_group(MATERIAL_GROUP, &self.materials[material], &[]);
            }
        };
        pass.set_index_buffer(lod.indices.slice(..), wgpu::IndexFormat::Uint32);
        if let Some(slots) = &self.mesh_selection[mesh].material_slots[level] {
            for slot in slots {
                let material = slot.material().raw() as usize;
                draw(pass, self.instances.pipelines[material], material);
                pass.draw_indexed(slot.indices(), 0, range.slots.clone());
                count.draw(instances);
            }
        } else {
            draw(pass, pipeline, material);
            pass.draw_indexed(0..lod.index_count, 0, range.slots.clone());
            count.draw(instances);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::BoundState;

    #[test]
    fn repeated_state_is_bound_once_and_a_pipeline_change_rebinds_the_material() {
        let mut state = BoundState::default();
        assert_eq!(state.bind(0, 4), (true, true));
        assert_eq!(state.bind(0, 4), (false, false));
        assert_eq!(state.bind(0, 5), (false, true));
        assert_eq!(state.bind(1, 5), (true, true));
        assert_eq!(state.bind(1, 5), (false, false));
        assert_eq!(state.bind(0, 5), (true, true));
    }
}
