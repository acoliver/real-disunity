#[cfg(test)]
mod test_hooks;
use std::cell::RefCell;
#[cfg(test)]
use std::ops::Index;
use std::ops::Range;

use wgpu::{BindGroup, Buffer};
#[cfg(test)]
use wgpu::{RenderPass, RenderPipeline};
use worldrunner_render_program::runtime::PassCommands;

use crate::object::layout::GeometryLayout;

use super::{
    DrawnObjects, FRAME_GROUP, FrameUniform, ObjectDraws, SceneGpu, SelectionPublication,
    selection_reuse,
    visibility::{SelectedRange, Selection},
};
#[cfg(test)]
use crate::upload::MATERIAL_GROUP;

/// Render state the current selection's draws have already set. wgpu validates and encodes
/// every state call again, including a pipeline change to the pipeline already bound, so
/// repeats are skipped. A pipeline change can invalidate the bound material group, so a
/// material is only reused under the pipeline it was bound with. A pipeline is the pipeline
/// index together with the vertex layout of the geometry it draws: the same index prepares a
/// different pipeline for a rigid level and for a skinned one.
#[derive(Debug, Default)]
pub(super) struct BoundState {
    pipeline: Option<(usize, GeometryLayout)>,
    material: Option<usize>,
}

impl BoundState {
    /// Records that `pipeline` for `geometry` and `material` are bound and returns which of
    /// them must be set.
    pub(super) fn bind(
        &mut self,
        pipeline: usize,
        geometry: GeometryLayout,
        material: usize,
    ) -> (bool, bool) {
        let set_pipeline = self.pipeline != Some((pipeline, geometry));
        if set_pipeline {
            self.pipeline = Some((pipeline, geometry));
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
    /// The vertex format of the level the draw reads, which selects the program and layout.
    pub geometry: GeometryLayout,
    /// The indices of the draw in the index buffer the walk named, which may hold other levels.
    pub indices: Range<u32>,
    /// The vertex a level's indices count from in the vertex buffer the walk named.
    pub base_vertex: i32,
    pub instances: Range<u32>,
}

/// Where the draws of a selection go. The walk over a selection says which buffers a draw reads
/// and in what order, and a sink decides how they reach the pass: the test-only `FixedSink` sets
/// each buffer as it is named, and the runtime sink draws a geometry over them.
pub(super) trait DrawSink {
    /// The instance buffer of the draws that follow, the previous-instance buffer when the
    /// batch has one, and the skin stream when the batch's mesh is skinned.
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>, skin: Option<&Buffer>);
    /// The vertex buffer of the level the next draws read.
    fn vertices(&mut self, vertices: &Buffer);
    /// The index buffer of the level the next draws read.
    fn indices(&mut self, indices: &Buffer);
    fn draw(&mut self, draw: Draw);
}

/// Records draws with render pipelines the caller built, one for each pipeline index, and a bind
/// group for each material. Only tests use it: the shadow-depth walks in `fade_empty` and
/// `curved_wind` and the foliage tests draw with pipelines they build themselves. No scene pass
/// draws this way.
#[cfg(test)]
pub(super) struct FixedSink<'a, P: ?Sized, I: ?Sized> {
    pass: &'a mut P,
    pipelines: &'a I,
    materials: &'a [BindGroup],
    state: BoundState,
}

#[cfg(test)]
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

#[cfg(test)]
impl<P, I> DrawSink for FixedSink<'_, P, I>
where
    P: PassCommands + ?Sized,
    I: Index<usize, Output = RenderPipeline> + ?Sized,
{
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>, _skin: Option<&Buffer>) {
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
            .set_index_buffer(indices, super::mesh_arena::INDEX_FORMAT);
    }

    fn draw(&mut self, draw: Draw) {
        // Every scene pass draws objects through the object programs; only tests record
        // through these fixed pipelines, which have no skinned layout.
        assert!(
            !draw.geometry.is_skinned(),
            "a fixed object route reached skinned geometry"
        );
        let (set_pipeline, set_material) =
            self.state.bind(draw.pipeline, draw.geometry, draw.material);
        if set_pipeline {
            self.pass.set_pipeline(&self.pipelines[draw.pipeline]);
        }
        if set_material {
            self.pass
                .set_bind_group(MATERIAL_GROUP, &self.materials[draw.material]);
        }
        self.pass
            .draw_indexed(draw.indices, draw.base_vertex, draw.instances);
    }
}

/// A view's selected objects and the frame they were selected for.
pub(super) struct ObjectView {
    pub(super) frame: FrameUniform,
    pub(super) published: std::rc::Rc<RefCell<std::rc::Rc<Selection>>>,
    pub(super) selection: std::rc::Rc<Selection>,
}

impl SceneGpu {
    /// Selects the view's objects and binds the frame group, which both ways of recording them
    /// do before the first draw.
    pub(super) fn begin_objects<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &BindGroup,
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) -> ObjectView {
        let owner = worldrunner_render_gpu::submission::Publications::current();
        assert!(
            self.instances.ready_for(owner.as_ref()),
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

    /// Publishes a shadow cascade's recorded draws. A camera pass's draws go to the caller,
    /// which knows the pass, with the selection to publish.
    pub(super) fn end_objects(
        &self,
        view: ObjectView,
        shadow_matrix: Option<(u32, glam::Mat4)>,
        objects: ObjectDraws,
    ) -> DrawnObjects {
        if let Some((index, _)) = shadow_matrix {
            self.pass_counters.shadows[index as usize].record(objects.total());
        }
        DrawnObjects {
            selection: SelectionPublication {
                destination: view.published,
                selection: view.selection,
            },
            objects,
        }
    }

    pub(super) fn record_lod_range(
        &self,
        sink: &mut impl DrawSink,
        range: &SelectedRange,
        drawn: &mut ObjectDraws,
    ) {
        let batch = &self.instances.batches[&range.key];
        sink.instances(
            batch.buffer.as_deref().expect("flushed instance buffer"),
            batch.previous.as_deref(),
            batch
                .skin
                .as_ref()
                .map(|stream| stream.buffer.as_deref().expect("flushed skin buffer")),
        );
        self.record_lod_draws(sink, range, drawn);
    }

    /// Draws the instances `range.slots` of the buffer the sink already holds as the instances,
    /// counting each draw in `drawn`.
    pub(super) fn record_lod_draws(
        &self,
        sink: &mut impl DrawSink,
        range: &SelectedRange,
        drawn: &mut ObjectDraws,
    ) {
        let (_, mesh, _) = range.key;
        let lod = &self.meshes[mesh].lods[range.lod];
        sink.vertices(lod.vertices.buffer());
        self.record_lod_slots(sink, range, (range.lod, lod.vertices.base_vertex()), drawn);
        if let Some(vertices) = &lod.transition {
            sink.vertices(vertices.buffer());
            // The complement belongs to the preceding level, including its materials.
            self.record_lod_slots(sink, range, (range.lod - 1, vertices.base_vertex()), drawn);
        }
    }

    fn record_lod_slots(
        &self,
        sink: &mut impl DrawSink,
        range: &SelectedRange,
        (level, base_vertex): (usize, i32),
        drawn: &mut ObjectDraws,
    ) {
        let instances = u64::from(range.slots.end - range.slots.start);
        let (pipeline, mesh, material) = range.key;
        let lod = &self.meshes[mesh].lods[level];
        sink.indices(lod.indices.buffer());
        let first = lod.indices.first();
        if let Some(slots) = &self.mesh_selection[mesh].material_slots[level] {
            for slot in slots {
                let material = slot.material().raw() as usize;
                sink.draw(Draw {
                    pipeline: self.instances.pipelines[material],
                    material,
                    geometry: lod.layout,
                    indices: first + slot.indices().start..first + slot.indices().end,
                    base_vertex,
                    instances: range.slots.clone(),
                });
                drawn.draw(self.resource_counters.impostors[material], instances);
            }
        } else {
            sink.draw(Draw {
                pipeline,
                material,
                geometry: lod.layout,
                indices: first..first + lod.index_count,
                base_vertex,
                instances: range.slots.clone(),
            });
            drawn.draw(self.resource_counters.impostors[material], instances);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BoundState;
    use crate::object::layout::GeometryLayout::{Skinned, Static};

    #[test]
    fn repeated_state_is_bound_once_and_a_pipeline_change_rebinds_the_material() {
        let mut state = BoundState::default();
        assert_eq!(state.bind(0, Static, 4), (true, true));
        assert_eq!(state.bind(0, Static, 4), (false, false));
        assert_eq!(state.bind(0, Static, 5), (false, true));
        assert_eq!(state.bind(1, Static, 5), (true, true));
        assert_eq!(state.bind(1, Static, 5), (false, false));
        assert_eq!(state.bind(0, Static, 5), (true, true));
    }

    #[test]
    fn a_change_of_vertex_layout_rebinds_the_pipeline_and_the_material() {
        let mut state = BoundState::default();
        assert_eq!(state.bind(0, Static, 4), (true, true));
        assert_eq!(state.bind(0, Skinned, 4), (true, true));
        assert_eq!(state.bind(0, Skinned, 4), (false, false));
    }
}
