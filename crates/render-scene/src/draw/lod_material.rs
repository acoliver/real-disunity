use super::{GpuLod, MATERIAL_GROUP, SceneGpu, visibility::SelectedRange};

impl SceneGpu {
    pub(super) fn record_lod_range(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        range: &SelectedRange,
    ) {
        let (_, mesh, _) = range.key;
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
        let lod = &self.meshes[mesh].lods[range.lod];
        pass.set_vertex_buffer(0, lod.vertices.slice(..));
        self.record_lod_slots(pass, pipelines, range, lod, range.lod);
        if let Some(vertices) = &lod.transition {
            let previous = &self.meshes[mesh].lods[range.lod - 1];
            pass.set_vertex_buffer(0, vertices.slice(..));
            // The complement belongs to the preceding level, including its materials.
            self.record_lod_slots(pass, pipelines, range, previous, range.lod - 1);
        }
    }

    fn record_lod_slots(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        range: &SelectedRange,
        lod: &GpuLod,
        level: usize,
    ) {
        let (pipeline, mesh, material) = range.key;
        pass.set_index_buffer(lod.indices.slice(..), wgpu::IndexFormat::Uint32);
        if let Some(slots) = &self.mesh_selection[mesh].material_slots[level] {
            for slot in slots {
                let material = slot.material().raw() as usize;
                pass.set_pipeline(&pipelines[self.instances.pipelines[material]]);
                pass.set_bind_group(MATERIAL_GROUP, &self.materials[material], &[]);
                pass.draw_indexed(slot.indices(), 0, range.slots.clone());
            }
        } else {
            pass.set_pipeline(&pipelines[pipeline]);
            pass.set_bind_group(MATERIAL_GROUP, &self.materials[material], &[]);
            pass.draw_indexed(0..lod.index_count, 0, range.slots.clone());
        }
    }
}
