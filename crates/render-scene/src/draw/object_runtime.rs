//! Records the forward objects through the program runtime.
//!
//! The walk over a view's selection is shared with the test-only fixed sink and key sink
//! ([`DrawSink`]); only how a draw reaches the pass differs. A change of pipeline or material
//! binds the prepared pipeline of the pair (the pipeline and the material's set), and every draw
//! is recorded once over the buffers the walk named, borrowed for the call. The pass records a
//! buffer when the walk names it, a pipeline when it differs from the one set, and the material's
//! group when the material changes.

use std::rc::Rc;

use wgpu::{Buffer, RenderPipeline};
use worldrunner_render_program::runtime::{
    BorrowedGeometry, PassCommands, PreparedPipeline, VertexLayoutSpec,
};

use super::lod_material::{BoundState, Draw, DrawSink};
use super::{DrawnObjects, SceneGpu, SelectionPublication};
use crate::object::layout::{GeometryLayout, vertex_layout};
use crate::object::prepared::PreparedObjects;
use crate::object::program::Context;

/// The buffers the walk has named since the last draw, which the next draw must set.
#[derive(Clone, Copy, Default)]
struct Named {
    instances: bool,
    vertices: bool,
    indices: bool,
}

/// The buffers of the instances the walk named: placements, previous placements and skin stream.
struct Instances {
    current: Buffer,
    previous: Option<Buffer>,
    skin: Option<Buffer>,
}

/// A pass that leaves out the commands a recording already issued.
///
/// A prepared pipeline binds its pipeline and groups and a geometry sets all of its buffers, so
/// each draw of a run would repeat what the one before it set. This sets a buffer only when the
/// walk has named a buffer for its slot since the last draw and it is not the buffer the slot
/// already holds, and a pipeline only when it is not the one already set. Ranges of one mesh
/// level under different materials name the same vertex and index buffers, so only the first
/// binds them. Pairs of one key and different materials share a render pipeline, so a material
/// change records the material's group alone. The cache lives as long as the sink, which owns
/// every command between its draws.
struct Elided<'a, P: ?Sized> {
    pass: &'a mut P,
    pipeline: Option<RenderPipeline>,
    named: Named,
    /// The buffer each vertex slot holds, by slot.
    vertex_slots: Vec<Option<Buffer>>,
    index: Option<(Buffer, wgpu::IndexFormat)>,
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
        if !named {
            return;
        }
        let index = slot as usize;
        if self.vertex_slots.len() <= index {
            self.vertex_slots.resize(index + 1, None);
        }
        let held = &mut self.vertex_slots[index];
        if held.as_ref() != Some(buffer) {
            self.pass.set_vertex_buffer(slot, buffer);
            *held = Some(buffer.clone());
        }
    }

    fn set_index_buffer(&mut self, buffer: &Buffer, format: wgpu::IndexFormat) {
        if !self.named.indices {
            return;
        }
        let held = self
            .index
            .as_ref()
            .is_some_and(|(held, held_format)| held == buffer && *held_format == format);
        if !held {
            self.pass.set_index_buffer(buffer, format);
            self.index = Some((buffer.clone(), format));
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
    layouts: [VertexLayoutSpec; 2],
    state: BoundState,
    current: Option<Rc<PreparedPipeline>>,
    instances: Option<Instances>,
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
                vertex_slots: Vec::new(),
                index: None,
            },
            objects,
            context,
            layouts: [GeometryLayout::Static, GeometryLayout::Skinned].map(|geometry| {
                vertex_layout(
                    context.previous_transforms() && !geometry.is_skinned(),
                    geometry,
                )
            }),
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
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>, skin: Option<&Buffer>) {
        self.instances = Some(Instances {
            current: current.clone(),
            previous: previous.cloned(),
            skin: skin.cloned(),
        });
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
        let (set_pipeline, set_material) = self.state.bind(index, draw.geometry, draw.material);
        if set_pipeline || set_material {
            let pipeline = self
                .objects
                .pipeline(self.context, (index, draw.material), draw.geometry)
                .expect("the object program prepares against the layouts of its context");
            pipeline.bind(&mut self.pass);
            self.current = Some(pipeline);
        }
        let pipeline = self.current.as_ref().expect("a draw binds its pipeline");
        let instances = self.instances.as_ref().expect("a draw names its instances");
        let vertices = self.vertices.as_ref().expect("a draw names its vertices");
        let skinned = draw.geometry.is_skinned();
        let stream = if skinned {
            // A compacted view names its instances once for groups of both layouts, and a
            // static group before this one never set the skin slot.
            self.pass.named.instances = true;
            Some(
                instances
                    .skin
                    .as_ref()
                    .expect("a skinned level has a skin stream"),
            )
        } else if self.context.previous_transforms() {
            Some(
                instances
                    .previous
                    .as_ref()
                    .expect("a pass that writes velocity tracks previous transforms"),
            )
        } else {
            None
        };
        let buffers: &[&Buffer] = match stream {
            Some(stream) => &[vertices, &instances.current, stream],
            None => &[vertices, &instances.current],
        };
        let indices = BorrowedGeometry::Indexed {
            buffer: self.indices.as_ref().expect("a draw names its indices"),
            format: wgpu::IndexFormat::Uint32,
            first: draw.indices.start,
            count: draw.indices.end - draw.indices.start,
            base_vertex: 0,
        };
        pipeline
            .draw_borrowed(
                &mut self.pass,
                &self.layouts[usize::from(skinned)],
                buffers,
                indices,
                draw.instances,
            )
            .expect("a selected range fits the buffers of the objects' vertex layout");
        self.pass.named = Named::default();
    }
}

/// Collects the pipeline index of every draw, after a mask, without recording anything.
#[cfg(test)]
struct KeySink {
    mask: usize,
    keys: std::collections::BTreeSet<usize>,
}

#[cfg(test)]
impl DrawSink for KeySink {
    fn instances(&mut self, _: &Buffer, _: Option<&Buffer>, _: Option<&Buffer>) {}

    fn vertices(&mut self, _: &Buffer) {}

    fn indices(&mut self, _: &Buffer) {}

    fn draw(&mut self, draw: Draw) {
        self.keys.insert(draw.pipeline ^ self.mask);
    }
}

impl SceneGpu {
    /// Records the scene's objects through the program runtime. The frame
    /// group is bound here; the shadow and screen groups are the pass's, bound before this.
    pub(crate) fn draw_objects_prepared<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &wgpu::BindGroup,
        context: &Context,
    ) -> DrawnObjects {
        let view = self.begin_objects(pass, frame_group, None);
        let mut sink = PreparedSink::new(pass, &self.objects, context);
        let objects = self.record_selection(&mut sink, (None, &view.frame), &view.selection.ranges);
        self.end_objects(view, None, objects)
    }

    /// Records the objects that `cascade` selects into its shadow map, with the caster programs of
    /// `context` and the cascade's light frame `frame_group` lent at set 0.
    pub(crate) fn draw_casters_prepared<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &wgpu::BindGroup,
        context: &Context,
        cascade: (u32, glam::Mat4),
    ) -> SelectionPublication {
        let shadow = Some(cascade);
        let view = self.begin_objects(pass, frame_group, shadow);
        let mut sink = PreparedSink::new(pass, &self.objects, context);
        let objects =
            self.record_selection(&mut sink, (shadow, &view.frame), &view.selection.ranges);
        self.end_objects(view, shadow, objects).selection
    }

    /// The group the object draws bind at set 0, for tests that record the objects into pass
    /// commands of their own.
    #[cfg(test)]
    pub(crate) fn drawing_frame_group_for_test(&self) -> wgpu::BindGroup {
        self.drawing_frame_group()
    }

    /// The keys, as indices into `PipelineKey::ALL` after `mask`, that the camera view's
    /// selection draws now. It runs the walk a pass records and keeps each draw's pipeline.
    #[cfg(test)]
    pub(crate) fn pipeline_keys_drawn(&self, mask: usize) -> std::collections::BTreeSet<usize> {
        let frame = self.drawing_uniform();
        let selection = self.select_objects(None, &frame);
        let mut sink = KeySink {
            mask,
            keys: std::collections::BTreeSet::new(),
        };
        self.record_selection(&mut sink, (None, &frame), &selection.ranges);
        sink.keys
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
    ) -> DrawnObjects {
        let view = self.begin_objects(pass, frame_group, None);
        let mut sink = PreparedSink::new(pass, &self.objects, context).with_mask(mask);
        let objects = self.record_selection(&mut sink, (None, &view.frame), &view.selection.ranges);
        self.end_objects(view, None, objects)
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
        self.prepare_object_context(selected.object_context())
    }

    /// Prepares the pipeline of every pair of pipeline key and material the scene's batches draw,
    /// for the programs of `context`, before a pass that draws with them opens.
    ///
    /// # Errors
    /// Fails when the runtime refuses the objects' program, set or pipeline for the context.
    pub(crate) fn prepare_object_context(
        &self,
        context: &Context,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        for (index, material, geometry) in self.drawn_pairs() {
            self.objects
                .pipeline(context, (index, material), geometry)
                .map_err(
                    |error| worldrunner_render_post::FrameError::FeaturePreparation {
                        feature: "objects",
                        reason: error.to_string(),
                    },
                )?;
        }
        Ok(())
    }

    /// Prepares the objects of a term replay drawn with `pipelines` in the frame being drawn,
    /// before the replay's pass opens.
    ///
    /// # Errors
    /// Fails when the runtime refuses the objects' program, set or pipeline for the context.
    pub(crate) fn prepare_objects_replay(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        self.prepare_objects(pipelines, &self.drawing_uniform())
    }

    /// The pairs of pipeline index and material, with the vertex format of the level that draws
    /// them, that the batches holding instances draw: a mesh level with material slots draws
    /// the pair of each slot, and one without draws the pair of the batch.
    fn drawn_pairs(&self) -> std::collections::BTreeSet<(usize, usize, GeometryLayout)> {
        let mut pairs = std::collections::BTreeSet::new();
        for (&(pipeline, mesh, material), batch) in &self.instances.batches {
            if !batch.has_instances() {
                continue;
            }
            for (level, slots) in self.mesh_selection[mesh].material_slots.iter().enumerate() {
                let geometry = self.meshes[mesh].lods[level].layout;
                match slots {
                    Some(slots) => pairs.extend(slots.iter().map(|slot| {
                        let material = slot.material().raw() as usize;
                        (self.instances.pipelines[material], material, geometry)
                    })),
                    None => {
                        pairs.insert((pipeline, material, geometry));
                    }
                }
            }
        }
        pairs
    }
}
