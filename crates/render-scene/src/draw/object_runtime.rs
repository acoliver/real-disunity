//! Records the forward objects through the program runtime.
//!
//! The walk over a view's selection is the fixed path's ([`DrawSink`]); only how a draw reaches
//! the pass differs. A change of pipeline or material binds the prepared pipeline of the pair
//! (the pipeline and the material's set), and every draw is one [`GeometryDraw`] over the
//! buffers the walk named. The pass records the commands the fixed path recorded: a buffer is
//! set when the walk names it, a pipeline when it differs from the one set, and the material's
//! group when the material changes.

use std::rc::Rc;

use wgpu::{Buffer, RenderPipeline};
use worldrunner_render_program::runtime::{
    GeometryDraw, ImportedGeometry, ImportedIndices, PassCommands, PreparedPipeline,
    VertexLayoutSpec,
};

use super::lod_material::{BoundState, Draw, DrawSink};
use super::{SceneGpu, SelectionPublication};
use crate::object::prepared::PreparedObjects;
use crate::object::program::{Context, vertex_layout};

/// The buffers the walk has named since the last draw, which the next draw must set.
#[derive(Clone, Copy, Default)]
struct Named {
    instances: bool,
    vertices: bool,
    indices: bool,
}

/// A pass that leaves out the commands a recording already issued.
///
/// A prepared pipeline binds its pipeline and groups and a geometry sets all of its buffers, so
/// each draw of a run would repeat what the one before it set. This sets a buffer only when the
/// walk has named a buffer for its slot since the last draw, and a pipeline only when it is not
/// the one already set. Pairs of one key and different materials share a render pipeline, so a
/// material change records the material's group alone.
struct Elided<'a, P: ?Sized> {
    pass: &'a mut P,
    pipeline: Option<RenderPipeline>,
    named: Named,
}

impl<P: PassCommands + ?Sized> PassCommands for Elided<'_, P> {
    fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        if self.pipeline.as_ref() != Some(pipeline) {
            self.pass.set_pipeline(pipeline);
            self.pipeline = Some(pipeline.clone());
        }
    }

    fn set_bind_group(&mut self, set: u32, group: &wgpu::BindGroup) {
        self.pass.set_bind_group(set, group);
    }

    fn set_vertex_buffer(&mut self, slot: u32, buffer: &Buffer) {
        let named = if slot == 0 {
            self.named.vertices
        } else {
            self.named.instances
        };
        if named {
            self.pass.set_vertex_buffer(slot, buffer);
        }
    }

    fn set_index_buffer(&mut self, buffer: &Buffer, format: wgpu::IndexFormat) {
        if self.named.indices {
            self.pass.set_index_buffer(buffer, format);
        }
    }

    fn draw(&mut self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>) {
        self.pass.draw(vertices, instances);
    }

    fn draw_indexed(
        &mut self,
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    ) {
        self.pass.draw_indexed(indices, base_vertex, instances);
    }
}

/// Records draws through the prepared pipelines of a context.
///
/// Pairs are prepared before the pass opens by [`SceneGpu::prepare_objects`]; a pair a draw finds
/// unprepared is made then.
pub(super) struct PreparedSink<'a, P: ?Sized> {
    pass: Elided<'a, P>,
    objects: &'a PreparedObjects,
    context: &'a Context,
    layout: VertexLayoutSpec,
    state: BoundState,
    current: Option<Rc<PreparedPipeline>>,
    instances: Option<(Buffer, Option<Buffer>)>,
    vertices: Option<Buffer>,
    indices: Option<Buffer>,
    /// Sends pipeline index `i` to the pipeline of key `i ^ mask`, as the capture tests do for
    /// the keys no material reaches.
    #[cfg(test)]
    mask: usize,
}

impl<'a, P: PassCommands + ?Sized> PreparedSink<'a, P> {
    pub(super) fn new(pass: &'a mut P, objects: &'a PreparedObjects, context: &'a Context) -> Self {
        Self {
            pass: Elided {
                pass,
                pipeline: None,
                named: Named::default(),
            },
            objects,
            context,
            layout: vertex_layout(context.previous_transforms()),
            state: BoundState::default(),
            current: None,
            instances: None,
            vertices: None,
            indices: None,
            #[cfg(test)]
            mask: 0,
        }
    }

    /// Sends pipeline index `i` to the pipeline of key `i ^ mask`.
    #[cfg(test)]
    pub(super) fn with_mask(mut self, mask: usize) -> Self {
        self.mask = mask;
        self
    }
}

impl<P: PassCommands + ?Sized> DrawSink for PreparedSink<'_, P> {
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>) {
        self.instances = Some((current.clone(), previous.cloned()));
        self.pass.named.instances = true;
    }

    fn vertices(&mut self, vertices: &Buffer) {
        self.vertices = Some(vertices.clone());
        self.pass.named.vertices = true;
    }

    fn indices(&mut self, indices: &Buffer) {
        self.indices = Some(indices.clone());
        self.pass.named.indices = true;
    }

    fn draw(&mut self, draw: Draw) {
        let index = draw.pipeline;
        #[cfg(test)]
        let index = index ^ self.mask;
        let (set_pipeline, set_material) = self.state.bind(index, draw.material);
        if set_pipeline || set_material {
            let pipeline = self
                .objects
                .pipeline(self.context, index, draw.material)
                .expect("the object program prepares against the layouts of its context");
            pipeline.bind(&mut self.pass);
            self.current = Some(pipeline);
        }
        let pipeline = self.current.as_ref().expect("a draw binds its pipeline");
        let (current, previous) = self.instances.as_ref().expect("a draw names its instances");
        let mut buffers = vec![
            self.vertices.clone().expect("a draw names its vertices"),
            current.clone(),
        ];
        if self.context.previous_transforms() {
            buffers.push(
                previous
                    .clone()
                    .expect("a pass that writes velocity tracks previous transforms"),
            );
        }
        let geometry = GeometryDraw::new(
            &self.layout,
            &buffers,
            &ImportedGeometry::Indexed(ImportedIndices {
                buffer: self.indices.clone().expect("a draw names its indices"),
                format: wgpu::IndexFormat::Uint32,
                first: draw.indices.start,
                count: draw.indices.end - draw.indices.start,
                base_vertex: 0,
            }),
            draw.instances,
        )
        .expect("the buffers of a selected range hold what it draws");
        pipeline
            .draw(&mut self.pass, &geometry)
            .expect("the geometry was made for the objects' vertex layout");
        self.pass.named = Named::default();
    }
}

impl SceneGpu {
    /// Records the scene's objects through the program runtime, as
    /// [`draw_objects`](Self::draw_objects) records them with the fixed pipelines. The frame
    /// group is bound here; the shadow and screen groups are the pass's, bound before this.
    pub(crate) fn draw_objects_prepared<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &wgpu::BindGroup,
        context: &Context,
    ) -> SelectionPublication {
        let view = self.begin_objects(pass, frame_group, None);
        let mut sink = PreparedSink::new(pass, &self.objects, context);
        let count = self.record_selection(&mut sink, (None, &view.frame), &view.selection.ranges);
        self.end_objects(view, None, count)
    }

    /// [`draw_objects_prepared`](Self::draw_objects_prepared), sending pipeline index `i` to the
    /// pipeline of key `i ^ mask`.
    #[cfg(test)]
    pub(crate) fn draw_objects_prepared_masked<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &wgpu::BindGroup,
        context: &Context,
        mask: usize,
    ) -> SelectionPublication {
        let view = self.begin_objects(pass, frame_group, None);
        let mut sink = PreparedSink::new(pass, &self.objects, context).with_mask(mask);
        let count = self.record_selection(&mut sink, (None, &view.frame), &view.selection.ranges);
        self.end_objects(view, None, count)
    }

    /// Prepares the pipelines of the pairs of pipeline key and material that the scene's batches
    /// draw in a pass drawn with `pipelines` in the frame `frame`, before the pass opens, where a
    /// refusal is still an error rather than a lost draw. The pipelines are selected for the
    /// frame's receiver, convention and winding as the draw selects them. A pass that was not
    /// prepared makes its pipelines when it first draws.
    ///
    /// # Errors
    /// Fails when the runtime refuses the objects' program, set or pipeline for the context.
    pub(crate) fn prepare_objects(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
        frame: &crate::frame::FrameUniform,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        let selected = self.selected_pipelines(pipelines, frame);
        let context = selected.object_context();
        for (index, material) in self.drawn_pairs() {
            self.objects
                .pipeline(context, index, material)
                .map_err(
                    |error| worldrunner_render_post::FrameError::FeaturePreparation {
                        feature: "objects",
                        reason: error.to_string(),
                    },
                )?;
        }
        Ok(())
    }

    /// The pairs of pipeline index and material the batches that hold instances draw: a mesh
    /// level with material slots draws the pair of each slot, and one without draws the pair of
    /// the batch.
    fn drawn_pairs(&self) -> std::collections::BTreeSet<(usize, usize)> {
        let mut pairs = std::collections::BTreeSet::new();
        for (&(pipeline, mesh, material), batch) in &self.instances.batches {
            if !batch.has_instances() {
                continue;
            }
            for slots in &self.mesh_selection[mesh].material_slots {
                match slots {
                    Some(slots) => pairs.extend(slots.iter().map(|slot| {
                        let material = slot.material().raw() as usize;
                        (self.instances.pipelines[material], material)
                    })),
                    None => {
                        pairs.insert((pipeline, material));
                    }
                }
            }
        }
        pairs
    }
}
