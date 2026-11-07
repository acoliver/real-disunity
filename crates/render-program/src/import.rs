//! Draws over resources another owner already created: imported bindings, borrowed vertex and
//! index buffers and an instance range.
//!
//! The runtime's own buffers and textures are created by handle and counted against its budget.
//! A renderer that already holds its geometry and textures should not copy them in. It lends
//! them to one draw instead, and keeps its own accounting. Nothing lent is counted by the
//! runtime, so a lent resource adds nothing to [`live_bytes`](crate::runtime::ProgramRuntime::live_bytes)
//! and a lender that wants it counted does that in its own budget.
//!
//! A lent resource must outlive every frame that records the draw. The wgpu handles in a draw
//! are reference counted and wgpu keeps a resource alive while a recorded command needs it.
//! What the lender keeps besides is the draw's `owners`: values the draw holds and every frame
//! that [`retain`](crate::runtime::PreparedProgramDraw::retain)s the draw holds in turn, through
//! the frame-owner mechanism the runtime's own resources use, until that frame is cancelled,
//! dropped or completed on the GPU.

use std::ops::Range;

use crate::binding::EngineSemantic;
use crate::error::ProgramError;
use crate::packet::{
    PipelineRequest, PreparedGeometry, PreparedProgramDraw, RecordableDraw, TargetSignature,
    VertexLayout, instance_end,
};
use crate::program::GraphicsProgram;
use crate::range::{Extent, Fetch, check_extent};
use crate::resources::{BoundResource, Keepalive, Pins};
use crate::runtime::ProgramRuntime;
use crate::state::RenderState;

/// A texture and the view of it a binding reads. The texture tells the runtime the format and
/// sample count to check the binding against.
#[derive(Clone, Debug)]
pub struct ImportedTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

/// A buffer, texture view or sampler an owner lends to a draw.
#[derive(Clone, Debug)]
pub enum ImportedResource {
    Buffer(wgpu::Buffer),
    Texture(ImportedTexture),
    Sampler(wgpu::Sampler),
}

impl ImportedResource {
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            Self::Buffer(_) => "an imported buffer",
            Self::Texture(_) => "an imported texture",
            Self::Sampler(_) => "an imported sampler",
        }
    }
}

/// A resource lent to the binding that the program sources from `semantic`.
#[derive(Clone, Debug)]
pub struct Import {
    pub semantic: EngineSemantic,
    pub resource: ImportedResource,
}

impl Import {
    #[must_use]
    pub fn buffer(semantic: EngineSemantic, buffer: wgpu::Buffer) -> Self {
        Self {
            semantic,
            resource: ImportedResource::Buffer(buffer),
        }
    }

    #[must_use]
    pub fn texture(
        semantic: EngineSemantic,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
    ) -> Self {
        Self {
            semantic,
            resource: ImportedResource::Texture(ImportedTexture { texture, view }),
        }
    }

    #[must_use]
    pub fn sampler(semantic: EngineSemantic, sampler: wgpu::Sampler) -> Self {
        Self {
            semantic,
            resource: ImportedResource::Sampler(sampler),
        }
    }
}

/// A borrowed vertex buffer and how the vertex stage reads it.
#[derive(Clone, Debug)]
pub struct ImportedVertexInput {
    pub buffer: wgpu::Buffer,
    pub stride: u64,
    pub step: wgpu::VertexStepMode,
    /// The shader locations the buffer feeds, with formats and byte offsets.
    pub attributes: Vec<wgpu::VertexAttribute>,
}

/// A borrowed index buffer and the indices a draw reads.
#[derive(Clone, Debug)]
pub struct ImportedIndices {
    pub buffer: wgpu::Buffer,
    pub format: wgpu::IndexFormat,
    pub first: u32,
    pub count: u32,
    pub base_vertex: i32,
}

/// Which vertices a draw over borrowed buffers reads.
#[derive(Clone, Debug)]
pub enum ImportedGeometry {
    Vertices { first: u32, count: u32 },
    Indexed(ImportedIndices),
}

/// One draw of triangles over resources lent to it.
///
/// `imports` fill the bindings the program sources from an engine semantic, `bindings` fill the
/// ones that name runtime handles, `vertex_inputs` and the geometry's index buffer are borrowed,
/// and `instances` is the range of instances drawn. An instance-rate vertex buffer must hold as
/// many strides as the range's end. `owners` are held by the draw and by every frame that
/// retains it.
#[derive(Clone)]
pub struct ImportedDraw<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    pub vertex_inputs: &'a [ImportedVertexInput],
    pub bindings: &'a [BoundResource],
    pub imports: &'a [Import],
    pub geometry: &'a ImportedGeometry,
    pub instances: Range<u32>,
    pub owners: &'a [Keepalive],
}

impl ProgramRuntime {
    /// Prepares `draw` for passes with the attachments of `signature`.
    ///
    /// The state describes each of the signature's colour attachments, and a colour attachment
    /// the fragment stage does not write has an empty write mask. Everything that can fail is
    /// checked here: recording the draw only sets state.
    ///
    /// # Errors
    /// Fails on a binding with no import, an import no binding takes or one given twice, an
    /// import that is not the kind of resource its binding is or a buffer smaller than the
    /// shader needs, a state that does not describe the signature's attachments, an instance
    /// range that ends before it starts, geometry that runs past a borrowed buffer, a lost
    /// device or a GPU validation error.
    pub fn prepare_imported_draw(
        &mut self,
        draw: &ImportedDraw<'_>,
        signature: TargetSignature,
    ) -> Result<PreparedProgramDraw, ProgramError> {
        self.authority.check_device()?;
        let colors = draw.state.check_targets(signature.color_count())?;
        if draw.instances.start > draw.instances.end {
            return Err(ProgramError::InstanceRange {
                start: draw.instances.start,
                end: draw.instances.end,
            });
        }
        let fetches: Vec<_> = draw
            .vertex_inputs
            .iter()
            .map(|input| Fetch {
                stride: input.stride,
                step: input.step,
                attributes: &input.attributes,
                len: input.buffer.size(),
            })
            .collect();
        check_extent(
            &fetches,
            extent(draw.geometry),
            instance_end(&draw.instances),
        )?;
        let layouts: Vec<_> = draw
            .vertex_inputs
            .iter()
            .map(|input| VertexLayout {
                stride: input.stride,
                step: input.step,
                attributes: &input.attributes,
            })
            .collect();
        let pipeline = self.render_pipeline(&PipelineRequest {
            program: draw.program,
            state: draw.state,
            layouts: &layouts,
            signature,
            colors: &colors,
        })?;
        let groups =
            self.bind_groups_importing(&draw.program.gpu, draw.bindings, &[], draw.imports)?;
        let shares = self.binding_shares(draw.bindings)?;
        let recordable = RecordableDraw {
            signature,
            pipeline,
            groups,
            vertex_buffers: draw
                .vertex_inputs
                .iter()
                .map(|input| input.buffer.clone())
                .collect(),
            geometry: prepared_geometry(draw.geometry),
            instances: draw.instances.clone(),
        };
        Ok(PreparedProgramDraw {
            pins: Pins::borrowing(&self.authority, shares, draw.owners.to_vec()),
            draw: recordable,
        })
    }
}

fn extent(geometry: &ImportedGeometry) -> Extent {
    match geometry {
        ImportedGeometry::Vertices { first, count } => Extent::Vertices {
            first: *first,
            count: *count,
        },
        ImportedGeometry::Indexed(indices) => Extent::Indexed {
            buffer_len: indices.buffer.size(),
            format: indices.format,
            first: indices.first,
            count: indices.count,
            base_vertex: indices.base_vertex,
        },
    }
}

fn prepared_geometry(geometry: &ImportedGeometry) -> PreparedGeometry {
    match geometry {
        ImportedGeometry::Vertices { first, count } => PreparedGeometry::Vertices {
            first: *first,
            count: *count,
        },
        ImportedGeometry::Indexed(indices) => PreparedGeometry::Indexed {
            buffer: indices.buffer.clone(),
            format: indices.format,
            first: indices.first,
            count: indices.count,
            base_vertex: indices.base_vertex,
        },
    }
}
