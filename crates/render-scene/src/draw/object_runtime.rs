//! Records the forward objects through the program runtime.
//!
//! The walk over a view's selection is shared with the test-only fixed sink and key sink
//! ([`DrawSink`]); only how a draw reaches the pass differs. A change of pipeline or material
//! binds the prepared pipeline of the pair (the pipeline and the material's set), and every draw
//! is recorded once over the buffers the walk named, borrowed for the call. The pass records a
//! buffer when the walk names it, a pipeline when it differs from the one set, and the material's
//! group when the material changes.
//!
//! A sink holds the walk's draws and records them grouped by pipeline and material: a batch is
//! one mesh in one material, so the walk alternates materials as it moves from mesh to mesh, and
//! each change binds a material group that wgpu validates texture by texture. Grouped, a pass
//! binds each material once, and a caster pass binds one group for the materials its stages read
//! alike ([`PreparedObjects::recorded_as`]). Grouped draws of one mesh level whose instances
//! follow one another in the buffer are recorded as one draw, which is what a caster pass over
//! a compacted view makes of the batches of one mesh in materials it reads alike. The objects of
//! a pass are opaque or masked, drawn with depth tests and without blending, so only the winner
//! of an exact depth tie between two draws depends on the order. Levels share the chunks of
//! buffers their upload packed them into (see [`super::mesh_arena`]), so the draws of a material
//! in one chunk set the vertex and index buffers once and differ in `first_index` and
//! `base_vertex`.

#[cfg(test)]
mod test_hooks;
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
use grouping::{BufferTable, GroupKey, Grouping, grouped};

mod grouping;

/// The buffers the walk has named since the last draw, which the next draw must set.
#[derive(Clone, Copy, Default)]
struct Named {
    instances: bool,
    vertices: bool,
    indices: bool,
}

impl Named {
    /// Every buffer named: the pass sets each one that differs from the one it holds.
    const ALL: Self = Self {
        instances: true,
        vertices: true,
        indices: true,
    };
}

/// The buffers of the instances the walk named: placements, previous placements and skin stream.
struct Instances {
    current: Buffer,
    previous: Option<Buffer>,
    skin: Option<Buffer>,
}

impl Instances {
    /// Whether `other` names the same previous-placement and skin streams, so that instances
    /// that follow one another in the current buffer are read alike from both.
    fn streams_equal(&self, other: &Self) -> bool {
        self.previous == other.previous && self.skin == other.skin
    }
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
///
/// A sink holds the draws the walk names until [`finish`](Self::finish) and records them grouped
/// by pipeline, material and buffers.
pub(super) struct PreparedSink<'a, P: ?Sized> {
    recorder: Recorder<'a, P>,
    scratch: &'a mut SinkScratch,
}

/// What a sink fills while it holds a pass's draws, which the scene keeps between passes so that
/// a pass reuses the storage the one before grew.
#[derive(Default)]
pub(super) struct SinkScratch {
    names: Names,
    /// The draws the pass holds until it finishes.
    held: Vec<Held>,
    keys: Vec<GroupKey>,
    grouping: Grouping,
}

impl std::fmt::Debug for SinkScratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SinkScratch").finish_non_exhaustive()
    }
}

impl SinkScratch {
    /// Forgets the pass's draws and the buffers it named, which end their hold on them.
    fn clear(&mut self) {
        self.names.clear();
        self.held.clear();
    }
}

/// What the walk has named for the draws that follow, as positions in the tables the sink
/// keeps: a draw holds three small numbers instead of three handles.
#[derive(Default)]
struct Names {
    /// The vertex, index and current-instance buffers the walk has named, once each. They are
    /// kept apart, since a walk alternates between naming vertices and indices, and each kind
    /// repeats itself far more than the kinds together.
    vertex_buffers: BufferTable,
    index_buffers: BufferTable,
    current_buffers: BufferTable,
    /// The instances each naming gave, with the number of their current buffer in
    /// `current_buffers`.
    instances: Vec<(Instances, u32)>,
    current_instances: Option<u32>,
    vertices: Option<u32>,
    indices: Option<u32>,
}

impl Names {
    fn clear(&mut self) {
        self.vertex_buffers.clear();
        self.index_buffers.clear();
        self.current_buffers.clear();
        self.instances.clear();
        self.current_instances = None;
        self.vertices = None;
        self.indices = None;
    }
}

/// A draw a pass holds, with the buffers the walk named for it.
struct Held {
    draw: Draw,
    /// Positions in the sink's `Names`.
    vertices: u32,
    indices: u32,
    instances: u32,
}

/// The pass, the pipelines and the state a draw is recorded with.
struct Recorder<'a, P: ?Sized> {
    pass: Elided<'a, P>,
    objects: &'a PreparedObjects,
    context: &'a Context,
    layouts: [&'static VertexLayoutSpec; 2],
    state: BoundState,
    current: Option<Rc<PreparedPipeline>>,
    /// Sends pipeline index `i` to the pipeline of key `i ^ mask`, as the capture tests do for
    /// the keys no material reaches.
    #[cfg(test)]
    mask: usize,
}

impl<'a, P: PassCommands + ?Sized> PreparedSink<'a, P> {
    pub(super) fn new(
        pass: &'a mut P,
        objects: &'a PreparedObjects,
        context: &'a Context,
        scratch: &'a mut SinkScratch,
    ) -> Self {
        scratch.clear();
        Self {
            recorder: Recorder {
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
                #[cfg(test)]
                mask: 0,
            },
            scratch,
        }
    }

    /// Records the draws the sink holds, those that share a pipeline, then a material, then
    /// buffers together and those that tie in the order the walk named them. Call it after the
    /// walk.
    pub(super) fn finish(mut self) {
        let SinkScratch {
            names,
            held,
            keys,
            grouping,
        } = &mut *self.scratch;
        let (vertex_ranks, index_ranks, current_ranks) = (
            names.vertex_buffers.ranks(),
            names.index_buffers.ranks(),
            names.current_buffers.ranks(),
        );
        keys.clear();
        keys.extend(held.iter().map(|held| GroupKey {
            pipeline: held.draw.pipeline,
            geometry: held.draw.geometry,
            material: held.draw.material,
            vertices: vertex_ranks[held.vertices as usize],
            base_vertex: held.draw.base_vertex,
            indices: index_ranks[held.indices as usize],
            instances: current_ranks[names.instances[held.instances as usize].1 as usize],
            first_index: held.draw.indices.start,
            index_count: held.draw.indices.end - held.draw.indices.start,
            first_instance: held.draw.instances.start,
        }));
        let mut order = grouped(keys, grouping).iter().copied();
        let mut next = order.next();
        while let Some(first) = next {
            let held_first = &held[first as usize];
            let mut draw = held_first.draw.clone();
            let instances = &names.instances[held_first.instances as usize].0;
            // Draws of one mesh level whose instances follow one another in the same buffers are
            // one draw of all their instances.
            next = order.next();
            while let Some(position) = next
                && keys[position as usize].continues(&keys[first as usize], draw.instances.end)
                && names.instances[held[position as usize].instances as usize]
                    .0
                    .streams_equal(instances)
            {
                draw.instances.end = held[position as usize].draw.instances.end;
                next = order.next();
            }
            // The walk's names say nothing about the order recorded here; the pass leaves out
            // what the buffers it holds already are.
            self.recorder.pass.named = Named::ALL;
            self.recorder.record(
                draw,
                names.vertex_buffers.get(held_first.vertices),
                names.index_buffers.get(held_first.indices),
                instances,
            );
        }
        self.scratch.clear();
    }

    /// Sends pipeline index `i` to the pipeline of key `i ^ mask`.
    #[cfg(test)]
    pub(super) fn with_mask(mut self, mask: usize) -> Self {
        self.recorder.mask = mask;
        self
    }
}

impl<P: ?Sized> Drop for PreparedSink<'_, P> {
    fn drop(&mut self) {
        debug_assert!(
            self.scratch.held.is_empty() || std::thread::panicking(),
            "a sink was dropped holding draws it never recorded"
        );
    }
}

impl<P: PassCommands + ?Sized> Recorder<'_, P> {
    /// Records one draw: binds its pipeline and material when they changed and draws it over
    /// the buffers, which the pass sets when they differ from the ones it holds.
    fn record(&mut self, draw: Draw, vertices: &Buffer, indices: &Buffer, instances: &Instances) {
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
            buffer: indices,
            format: super::mesh_arena::INDEX_FORMAT,
            first: draw.indices.start,
            count: draw.indices.end - draw.indices.start,
            base_vertex: draw.base_vertex,
        };
        pipeline
            .draw_borrowed(
                &mut self.pass,
                self.layouts[usize::from(skinned)],
                buffers,
                indices,
                draw.instances,
            )
            .expect("a selected range fits the buffers of the objects' vertex layout");
        self.pass.named = Named::default();
    }
}

impl<P: PassCommands + ?Sized> DrawSink for PreparedSink<'_, P> {
    fn instances(&mut self, current: &Buffer, previous: Option<&Buffer>, skin: Option<&Buffer>) {
        let names = &mut self.scratch.names;
        let current_buffer = names.current_buffers.number(current);
        names.instances.push((
            Instances {
                current: current.clone(),
                previous: previous.cloned(),
                skin: skin.cloned(),
            },
            current_buffer,
        ));
        names.current_instances =
            Some(u32::try_from(names.instances.len() - 1).expect("few namings"));
        self.recorder.pass.named.instances = true;
    }

    fn vertices(&mut self, vertices: &Buffer) {
        self.scratch.names.vertices = Some(self.scratch.names.vertex_buffers.number(vertices));
        self.recorder.pass.named.vertices = true;
    }

    fn indices(&mut self, indices: &Buffer) {
        self.scratch.names.indices = Some(self.scratch.names.index_buffers.number(indices));
        self.recorder.pass.named.indices = true;
    }

    fn draw(&mut self, mut draw: Draw) {
        let recorded =
            self.recorder
                .objects
                .recorded_as(self.recorder.context, draw.pipeline, draw.material);
        (draw.pipeline, draw.material) = recorded;
        let names = &self.scratch.names;
        let instances = names.current_instances.expect("a draw names its instances");
        let vertices = names.vertices.expect("a draw names its vertices");
        let indices = names.indices.expect("a draw names its indices");
        self.scratch.held.push(Held {
            draw,
            vertices,
            indices,
            instances,
        });
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
        let mut scratch = self.sink_scratch.borrow_mut();
        let mut sink = PreparedSink::new(pass, &self.objects, context, &mut scratch);
        let objects = self.record_selection(&mut sink, (None, &view.frame), &view.selection.ranges);
        sink.finish();
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
        let mut scratch = self.sink_scratch.borrow_mut();
        let mut sink = PreparedSink::new(pass, &self.objects, context, &mut scratch);
        let objects =
            self.record_selection(&mut sink, (shadow, &view.frame), &view.selection.ranges);
        sink.finish();
        self.end_objects(view, shadow, objects).selection
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
        self.objects
            .prepare_pairs(context, &self.drawn_pairs())
            .map_err(
                |error| worldrunner_render_post::FrameError::FeaturePreparation {
                    feature: "objects",
                    reason: error.to_string(),
                },
            )
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
    /// the pair of each slot, and one without draws the pair of the batch. They change only with
    /// the placements and the tables, so the last answer stands until those change.
    pub(super) fn drawn_pairs(&self) -> Rc<[DrawnPair]> {
        let stamp = self.instances.layout_stamp(self.mesh_selection.len());
        let mut held = self.drawn_pair_memo.borrow_mut();
        if let Some((at, pairs)) = &held.0
            && *at == stamp
        {
            return pairs.clone();
        }
        let pairs: Rc<[_]> = self.collect_drawn_pairs().into_iter().collect();
        held.0 = Some((stamp, pairs.clone()));
        pairs
    }

    pub(super) fn collect_drawn_pairs(
        &self,
    ) -> std::collections::BTreeSet<(usize, usize, GeometryLayout)> {
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

/// A pipeline index, a material and the vertex format of the level drawn with them.
pub(super) type DrawnPair = (usize, usize, GeometryLayout);

/// The pairs the batches last drew, and the state of the placements and tables they were found
/// in.
#[derive(Debug, Default)]
pub(super) struct DrawnPairs(Option<(crate::instances::LayoutStamp, Rc<[DrawnPair]>)>);
