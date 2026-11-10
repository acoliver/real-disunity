use std::cell::RefCell;
use std::ops::{Index, Range};

use wgpu::{BindGroup, Buffer, RenderPass, RenderPipeline};
use worldrunner_render_program::runtime::PassCommands;

use super::{
    DrawCount, FRAME_GROUP, FrameUniform, MATERIAL_GROUP, SceneGpu, SelectionPublication,
    selection_reuse,
    visibility::{SelectedRange, Selection},
};

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
    pub(super) fn bind(&mut self, pipeline: usize, material: usize) -> (bool, bool) {
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

/// One indexed draw of a selected range: the pipeline and material of a material slot, the
/// indices it draws and the instances it draws them for.
#[derive(Clone, Debug)]
pub(super) struct Draw {
    pub pipeline: usize,
    pub material: usize,
    pub indices: Range<u32>,
    pub instances: Range<u32>,
}

/// Where the draws of a selection go. The walk over a selection says which buffers a draw reads
/// and in what order, and a sink decides how they reach the pass: the fixed path sets each
/// buffer as it is named, and the runtime path draws a geometry over them.
pub(super) trait DrawSink {
    /// The instance buffer of the draws that follow, and the previous-instance buffer when the
    /// batch has one.
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>);
    /// The vertex buffer of the level the next draws read.
    fn vertices(&mut self, vertices: &Buffer);
    /// The index buffer of the level the next draws read.
    fn indices(&mut self, indices: &Buffer);
    fn draw(&mut self, draw: Draw);
}

/// Records the fixed pipelines' draws: a render pipeline for each key and a bind group for each
/// material.
pub(super) struct FixedSink<'a, P: ?Sized, I: ?Sized> {
    pass: &'a mut P,
    pipelines: &'a I,
    materials: &'a [BindGroup],
    state: BoundState,
}

impl<'a, P: ?Sized, I: ?Sized> FixedSink<'a, P, I> {
    pub(super) fn new(pass: &'a mut P, pipelines: &'a I, materials: &'a [BindGroup]) -> Self {
        Self {
            pass,
            pipelines,
            materials,
            state: BoundState::default(),
        }
    }
}

impl<P, I> DrawSink for FixedSink<'_, P, I>
where
    P: PassCommands + ?Sized,
    I: Index<usize, Output = RenderPipeline> + ?Sized,
{
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>) {
        self.pass.set_vertex_buffer(1, current);
        // Only velocity pipelines declare this buffer; the others ignore the extra binding.
        if let Some(previous) = previous {
            self.pass.set_vertex_buffer(2, previous);
        }
    }

    fn vertices(&mut self, vertices: &Buffer) {
        self.pass.set_vertex_buffer(0, vertices);
    }

    fn indices(&mut self, indices: &Buffer) {
        self.pass
            .set_index_buffer(indices, wgpu::IndexFormat::Uint32);
    }

    fn draw(&mut self, draw: Draw) {
        let (set_pipeline, set_material) = self.state.bind(draw.pipeline, draw.material);
        if set_pipeline {
            self.pass.set_pipeline(&self.pipelines[draw.pipeline]);
        }
        if set_material {
            self.pass
                .set_bind_group(MATERIAL_GROUP, &self.materials[draw.material]);
        }
        self.pass.draw_indexed(draw.indices, 0, draw.instances);
    }
}

/// A view's selected objects and the frame they were selected for.
pub(super) struct ObjectView {
    pub(super) frame: FrameUniform,
    pub(super) published: std::rc::Rc<RefCell<Selection>>,
    pub(super) selection: Selection,
}

impl SceneGpu {
    /// Records the view's objects with fixed render pipelines, one for each pipeline index:
    /// the shadow casters', the metadata prepass's and the capture terms'.
    pub(crate) fn draw_objects(
        &self,
        pass: &mut RenderPass<'_>,
        frame_group: &BindGroup,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) -> SelectionPublication {
        let view = self.begin_objects(pass, frame_group, shadow_matrix);
        let mut sink = FixedSink::new(pass, pipelines, &self.materials);
        let count = self.record_selection(
            &mut sink,
            (shadow_matrix, &view.frame),
            &view.selection.ranges,
        );
        self.end_objects(view, shadow_matrix, count)
    }

    /// Selects the view's objects and binds the frame group, which both ways of recording them
    /// do before the first draw.
    pub(super) fn begin_objects<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &BindGroup,
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) -> ObjectView {
        assert!(
            self.instances
                .batches
                .values()
                .all(|batch| !batch.has_pending()),
            "flush instance updates before drawing"
        );
        let frame = self
            .drawing_frame
            .borrow()
            .as_ref()
            .and_then(|(owner, frame)| {
                worldrunner_render_gpu::submission::Publications::current()
                    .filter(|current| current.same_transaction(owner))
                    .map(|_| *frame)
            })
            .or_else(|| self.frame.get())
            .expect("set a frame before drawing");
        let published = self.views[selection_reuse::view_index(shadow_matrix)]
            .published
            .clone();
        let selection = self.select_objects(shadow_matrix, &frame);
        pass.set_bind_group(FRAME_GROUP, frame_group);
        ObjectView {
            frame,
            published,
            selection,
        }
    }

    /// Counts what was recorded for a shadow cascade and publishes the selection.
    pub(super) fn end_objects(
        &self,
        view: ObjectView,
        shadow_matrix: Option<(u32, glam::Mat4)>,
        count: DrawCount,
    ) -> SelectionPublication {
        if let Some((index, _)) = shadow_matrix {
            self.pass_counters.shadows[index as usize].record(count);
        }
        SelectionPublication {
            destination: view.published,
            selection: view.selection,
        }
    }

    pub(super) fn record_lod_range(
        &self,
        sink: &mut impl DrawSink,
        range: &SelectedRange,
    ) -> DrawCount {
        let batch = &self.instances.batches[&range.key];
        sink.instances(
            batch.buffer.as_ref().expect("flushed instance buffer"),
            batch.previous.as_ref(),
        );
        self.record_lod_draws(sink, range)
    }

    /// Draws the instances `range.slots` of the buffer the sink already holds as the instances.
    pub(super) fn record_lod_draws(
        &self,
        sink: &mut impl DrawSink,
        range: &SelectedRange,
    ) -> DrawCount {
        let (_, mesh, _) = range.key;
        let lod = &self.meshes[mesh].lods[range.lod];
        sink.vertices(&lod.vertices);
        let mut count = self.record_lod_slots(sink, range, range.lod);
        if let Some(vertices) = &lod.transition {
            sink.vertices(vertices);
            // The complement belongs to the preceding level, including its materials.
            count.add(self.record_lod_slots(sink, range, range.lod - 1));
        }
        count
    }

    fn record_lod_slots(
        &self,
        sink: &mut impl DrawSink,
        range: &SelectedRange,
        level: usize,
    ) -> DrawCount {
        let mut count = DrawCount::default();
        let instances = u64::from(range.slots.end - range.slots.start);
        let (pipeline, mesh, material) = range.key;
        let lod = &self.meshes[mesh].lods[level];
        sink.indices(&lod.indices);
        if let Some(slots) = &self.mesh_selection[mesh].material_slots[level] {
            for slot in slots {
                let material = slot.material().raw() as usize;
                sink.draw(Draw {
                    pipeline: self.instances.pipelines[material],
                    material,
                    indices: slot.indices(),
                    instances: range.slots.clone(),
                });
                count.draw(instances);
            }
        } else {
            sink.draw(Draw {
                pipeline,
                material,
                indices: 0..lod.index_count,
                instances: range.slots.clone(),
            });
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
