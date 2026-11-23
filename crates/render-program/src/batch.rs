//! One pipeline recorded over many geometries.

use std::borrow::Borrow;
use std::ops::Range;

use crate::error::ProgramError;
use crate::import::{BorrowedGeometry, Import, ImportedGeometry, prepared_geometry};
use crate::packet::{
    Bound, PipelineRequest, PreparedGeometry, TargetSignature, VertexLayout, instance_end,
};
use crate::pipeline_key::Key;
use crate::primitive::Primitive;
use crate::program::GraphicsProgram;
use crate::range::{Fetch, check_extent};
use crate::resources::{BoundResource, Keepalive, Pins};
use crate::runtime::ProgramRuntime;
use crate::set::{PreparedSet, held_by};
use crate::state::RenderState;

/// The commands a recording issues into a pass.
pub trait PassCommands {
    fn set_pipeline(&mut self, pipeline: &wgpu::RenderPipeline);
    fn set_bind_group(&mut self, set: u32, group: &wgpu::BindGroup);
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &wgpu::Buffer);
    fn set_index_buffer(&mut self, buffer: &wgpu::Buffer, format: wgpu::IndexFormat);
    fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>);
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>);
}

impl PassCommands for wgpu::RenderPass<'_> {
    fn set_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        Self::set_pipeline(self, pipeline);
    }
    fn set_bind_group(&mut self, set: u32, group: &wgpu::BindGroup) {
        Self::set_bind_group(self, set, group, &[]);
    }
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &wgpu::Buffer) {
        Self::set_vertex_buffer(self, slot, buffer.slice(..));
    }
    fn set_index_buffer(&mut self, buffer: &wgpu::Buffer, format: wgpu::IndexFormat) {
        Self::set_index_buffer(self, buffer.slice(..), format);
    }
    fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
        Self::draw(self, vertices, instances);
    }
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
        Self::draw_indexed(self, indices, base_vertex, instances);
    }
}

/// How one vertex buffer is read.
#[derive(Clone, Debug, PartialEq)]
pub struct VertexBufferSpec {
    pub stride: u64,
    pub step: wgpu::VertexStepMode,
    pub attributes: Vec<wgpu::VertexAttribute>,
}

/// The content of a vertex layout, so that two layouts built separately from equal buffers are
/// one layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VertexLayoutId(blake3::Hash);

impl VertexLayoutId {
    pub(crate) fn of(layouts: &[VertexLayout<'_>]) -> Self {
        let mut key = Key::new("vertex layout");
        for layout in layouts {
            key.value(&(layout.stride, layout.step, layout.attributes));
        }
        Self(key.finish())
    }
}

/// How a pipeline reads its vertex buffers, buffer by buffer in slot order.
///
/// A [`GeometryDraw`] is made for a layout and a pipeline is prepared for one, and the pipeline
/// draws only geometry made for an equal layout.
#[derive(Clone, Debug, PartialEq)]
pub struct VertexLayoutSpec {
    buffers: Vec<VertexBufferSpec>,
    id: VertexLayoutId,
}

impl VertexLayoutSpec {
    #[must_use]
    pub fn new(buffers: Vec<VertexBufferSpec>) -> Self {
        let views: Vec<_> = buffers
            .iter()
            .map(|buffer| VertexLayout {
                stride: buffer.stride,
                step: buffer.step,
                attributes: &buffer.attributes,
            })
            .collect();
        let id = VertexLayoutId::of(&views);
        Self { buffers, id }
    }

    #[must_use]
    pub fn buffers(&self) -> &[VertexBufferSpec] {
        &self.buffers
    }

    fn views(&self) -> Vec<VertexLayout<'_>> {
        self.buffers
            .iter()
            .map(|buffer| VertexLayout {
                stride: buffer.stride,
                step: buffer.step,
                attributes: &buffer.attributes,
            })
            .collect()
    }
}

/// One draw over buffers: the vertex buffers, the indices or vertices to draw and the range of
/// instances, with the layout the buffers were checked against.
///
/// It is plain data with no pipeline, runtime or device behind it, so a renderer makes one when
/// it uploads a mesh and keeps it as long as the mesh. A renderer that draws something once, and
/// would make one only to record it, calls [`PreparedPipeline::draw_borrowed`] instead.
#[derive(Clone, Debug)]
pub struct GeometryDraw {
    layout: VertexLayoutId,
    vertex_buffers: Vec<wgpu::Buffer>,
    geometry: PreparedGeometry,
    instances: Range<u32>,
}

impl GeometryDraw {
    /// The draw of `geometry` over `vertex`, which `layout` describes buffer by buffer, for the
    /// instances in `instances`. An instance-rate buffer must hold as many strides as the range
    /// ends at.
    ///
    /// # Errors
    /// Fails, naming the buffer and the byte or index, on a count of buffers other than the
    /// layout's, an instance range that ends before it starts and geometry or instances that run
    /// past a buffer.
    pub fn new(
        layout: &VertexLayoutSpec,
        vertex: &[wgpu::Buffer],
        geometry: &ImportedGeometry,
        instances: Range<u32>,
    ) -> Result<Self, ProgramError> {
        check_draw(layout, vertex, BorrowedGeometry::from(geometry), &instances)?;
        Ok(Self::from_parts(
            layout.id,
            vertex.to_vec(),
            prepared_geometry(geometry),
            instances,
        ))
    }

    /// A draw whose extent a caller has already checked.
    pub(crate) fn from_parts(
        layout: VertexLayoutId,
        vertex_buffers: Vec<wgpu::Buffer>,
        geometry: PreparedGeometry,
        instances: Range<u32>,
    ) -> Self {
        Self {
            layout,
            vertex_buffers,
            geometry,
            instances,
        }
    }

    pub(crate) fn record<P: PassCommands + ?Sized>(&self, pass: &mut P) {
        record_draw(
            pass,
            &self.vertex_buffers,
            BorrowedGeometry::from(&self.geometry),
            self.instances.clone(),
        );
    }
}

/// The checks that a draw of `geometry` over `vertex`, which `layout` describes buffer by buffer,
/// is well formed and fits its buffers. A [`GeometryDraw`] makes them once and
/// [`PreparedPipeline::draw_borrowed`] makes them for each draw, so the two refuse the same draws
/// with the same errors. It allocates nothing.
fn check_draw<B: Borrow<wgpu::Buffer>>(
    layout: &VertexLayoutSpec,
    vertex: &[B],
    geometry: BorrowedGeometry<'_>,
    instances: &Range<u32>,
) -> Result<(), ProgramError> {
    if vertex.len() != layout.buffers.len() {
        return Err(ProgramError::VertexBufferCount {
            expected: layout.buffers.len(),
            found: vertex.len(),
        });
    }
    if instances.start > instances.end {
        return Err(ProgramError::InstanceRange {
            start: instances.start,
            end: instances.end,
        });
    }
    let fetches = layout
        .buffers
        .iter()
        .zip(vertex)
        .map(|(spec, buffer)| Fetch {
            stride: spec.stride,
            step: spec.step,
            attributes: &spec.attributes,
            len: buffer.borrow().size(),
        });
    check_extent(fetches, geometry.extent(), instance_end(instances))
}

/// Sets the vertex buffers in slot order, then the index buffer of indexed geometry, and draws.
fn record_draw<'b, P: PassCommands + ?Sized>(
    pass: &mut P,
    vertex: impl IntoIterator<Item = &'b wgpu::Buffer>,
    geometry: BorrowedGeometry<'_>,
    instances: Range<u32>,
) {
    for (slot, buffer) in (0..).zip(vertex) {
        pass.set_vertex_buffer(slot, buffer);
    }
    match geometry {
        BorrowedGeometry::Vertices { first, count } => {
            pass.draw(first..first + count, instances);
        }
        BorrowedGeometry::Indexed {
            buffer,
            format,
            first,
            count,
            base_vertex,
        } => {
            pass.set_index_buffer(buffer, format);
            pass.draw_indexed(first..first + count, base_vertex, instances);
        }
    }
}

/// What a pipeline is prepared from: a draw over lent resources without its geometry.
///
/// `layout` is how the pipeline reads vertex buffers. `sets` are prepared sets, `bindings` and
/// `imports` fill the sets the program owns that are not prepared, and the sets the pass owner
/// binds stay with the pass owner. `owners` are held by the pipeline and by every frame that
/// retains it.
#[derive(Clone)]
pub struct PipelineDraw<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    pub layout: &'a VertexLayoutSpec,
    pub bindings: &'a [BoundResource],
    pub imports: &'a [Import],
    pub sets: &'a [&'a PreparedSet],
    pub owners: &'a [Keepalive],
}

/// A pipeline with the bind groups of its program's own sets, ready to record any number of
/// geometries into passes with the attachments of its [`signature`](Self::signature).
///
/// Recording is `bind` once, then `draw` for each geometry: `bind` sets the pipeline and the
/// groups, `draw` sets the geometry's vertex and index buffers and draws. A pass with C geometries
/// records `1 + groups + C * (buffers + 2)` commands for indexed geometry, where one draw per
/// geometry would record `C * (1 + groups + buffers + 2)`. Nothing here touches the runtime, so a
/// steady-state frame builds nothing.
///
/// The pipeline holds its owners, and so do the prepared sets it was made with. Every frame that
/// calls [`retain`](Self::retain), once however many geometries it draws, holds them until the
/// frame is cancelled, dropped or completed on the GPU. The wgpu buffers of a geometry are
/// reference counted and wgpu keeps them for as long as a recorded command needs them.
#[derive(Debug)]
pub struct PreparedPipeline {
    bound: Bound,
    layout: VertexLayoutId,
    pins: Pins,
}

impl PreparedPipeline {
    /// The attachments of the passes this pipeline can be recorded into.
    #[must_use]
    pub fn signature(&self) -> TargetSignature {
        self.bound.signature
    }

    /// The sets that the pass owner binds and this pipeline does not.
    #[must_use]
    pub fn external_sets(&self) -> &[u32] {
        &self.bound.external
    }

    /// Sets the pipeline and the groups of the sets the runtime built. Call it once before the
    /// geometries, and again after anything else in the pass has set another pipeline.
    pub fn bind<P: PassCommands + ?Sized>(&self, pass: &mut P) {
        self.bound.bind(pass);
    }

    /// Sets the geometry's buffers and draws it, with the pipeline bound.
    ///
    /// # Errors
    /// Fails with `GeometryLayoutMismatch`, before any command is recorded, when the geometry
    /// was made for another vertex layout than the pipeline reads.
    pub fn draw<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        geometry: &GeometryDraw,
    ) -> Result<(), ProgramError> {
        self.check_layout(geometry.layout)?;
        geometry.record(pass);
        Ok(())
    }

    /// Checks and records one draw that is made and forgotten in this call, with the pipeline
    /// bound: what [`GeometryDraw::new`] and [`draw`](Self::draw) do for a geometry that is kept,
    /// with the buffers and the index buffer borrowed. It allocates nothing and clones no
    /// buffer, so a renderer that draws each object once pays nothing for the draw but its checks.
    ///
    /// `vertex` holds one buffer for each buffer of `layout`, which must be the layout the
    /// pipeline reads.
    ///
    /// # Errors
    /// Fails as [`GeometryDraw::new`] does on a count of buffers other than the layout's, an
    /// instance range that ends before it starts and geometry or instances that run past a
    /// buffer, and then as [`draw`](Self::draw) does with `GeometryLayoutMismatch` on a layout
    /// other than the pipeline's. Nothing is recorded when it fails.
    pub fn draw_borrowed<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        layout: &VertexLayoutSpec,
        vertex: &[&wgpu::Buffer],
        geometry: BorrowedGeometry<'_>,
        instances: Range<u32>,
    ) -> Result<(), ProgramError> {
        check_draw(layout, vertex, geometry, &instances)?;
        self.check_layout(layout.id)?;
        record_draw(pass, vertex.iter().copied(), geometry, instances);
        Ok(())
    }

    fn check_layout(&self, layout: VertexLayoutId) -> Result<(), ProgramError> {
        if layout == self.layout {
            Ok(())
        } else {
            Err(ProgramError::GeometryLayoutMismatch)
        }
    }

    /// Makes the frame `encoder` records hold the pipeline's owners and its sets' until the
    /// frame is cancelled, dropped or completed on the GPU. Call it once for every frame the
    /// pipeline is recorded into.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority
    /// than the runtime's and when `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.pins.retain(encoder)
    }
}

impl ProgramRuntime {
    /// Prepares the pipeline of `draw` for passes with the attachments of `signature`, building
    /// the groups of the program's sets that `draw` does not give prepared and that the pass
    /// owner does not bind.
    ///
    /// The state describes each of the signature's colour attachments, as for
    /// [`prepare_imported_draw`](Self::prepare_imported_draw). A pipeline is not a draw, so it
    /// adds to `RuntimeStats` the module, pipeline and bind groups it builds and no draw.
    ///
    /// # Errors
    /// Fails on a state that does not describe the signature's attachments, a prepared set,
    /// import or bound resource that does not fit the program, a lost device or a GPU validation
    /// error such as a layout that does not match the shader's inputs.
    pub fn prepare_pipeline(
        &mut self,
        draw: &PipelineDraw<'_>,
        signature: TargetSignature,
    ) -> Result<PreparedPipeline, ProgramError> {
        self.authority.check_device()?;
        let colors = draw
            .state
            .check_targets(signature.color_count(), signature.samples())?;
        let layouts = draw.layout.views();
        let pipeline = self.render_pipeline(&PipelineRequest {
            program: draw.program,
            state: draw.state,
            layouts: &layouts,
            signature,
            colors: &colors,
            primitive: Primitive::TRIANGLE_LIST,
        })?;
        let groups = self.bind_groups_sharing(
            &draw.program.gpu,
            (draw.bindings, &[], draw.imports),
            draw.sets,
        )?;
        let shares = self.binding_shares(draw.bindings)?;
        Ok(PreparedPipeline {
            bound: Bound {
                signature,
                pipeline,
                groups,
                external: draw.program.gpu.external_sets.clone(),
            },
            layout: draw.layout.id,
            pins: Pins::borrowing(&self.authority, shares, held_by(draw.owners, draw.sets)),
        })
    }
}
