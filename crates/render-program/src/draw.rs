//! Draws of a graphics program into a render target.

use crate::error::ProgramError;
use crate::packet::TargetSignature;
use crate::program::GraphicsProgram;
use crate::resources::{BoundResource, BufferHandle, TargetEntry, TargetHandle};
use crate::runtime::ProgramRuntime;
use crate::state::RenderState;

/// A vertex buffer and how the vertex stage reads it.
#[derive(Clone, Debug)]
pub struct VertexInput {
    pub buffer: BufferHandle,
    pub stride: u64,
    pub step: wgpu::VertexStepMode,
    /// The shader locations the buffer feeds, with formats and byte offsets.
    pub attributes: Vec<wgpu::VertexAttribute>,
}

/// Which vertices a draw reads. A range that overflows or reaches past a bound buffer is refused
/// by name when the draw is prepared; the values inside an index buffer are not checked.
#[derive(Clone, Copy, Debug)]
pub enum Geometry {
    Vertices {
        first: u32,
        count: u32,
    },
    Indexed {
        buffer: BufferHandle,
        format: wgpu::IndexFormat,
        first: u32,
        count: u32,
        base_vertex: i32,
    },
}

/// One draw of triangles. The target keeps its contents, so clear it first with
/// [`ProgramRuntime::clear_target`] when it should start empty.
#[derive(Clone, Copy)]
pub struct DrawRequest<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub target: TargetHandle,
    pub vertex_inputs: &'a [VertexInput],
    pub bindings: &'a [BoundResource],
    pub geometry: Geometry,
}

impl ProgramRuntime {
    /// Records a fill of a target's color with `color` and its depth, when it has one, with
    /// `depth`.
    ///
    /// # Errors
    /// Fails on an unknown handle, an encoder that is not the authority's, a lost device or a
    /// GPU validation error.
    pub fn clear_target(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: TargetHandle,
        color: [f64; 4],
        depth: f32,
    ) -> Result<(), ProgramError> {
        self.recording(encoder)?;
        let target = self.store.target(target)?;
        self.guarded(|| {
            let attachment = wgpu::RenderPassColorAttachment {
                view: &target.color.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: color[0],
                        g: color[1],
                        b: color[2],
                        a: color[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
            };
            let depth_attachment =
                target
                    .depth
                    .as_ref()
                    .map(|d| wgpu::RenderPassDepthStencilAttachment {
                        view: &d.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(depth),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    });
            drop(
                encoder.begin_render_pass(&pass_descriptor(&[Some(attachment)], depth_attachment)),
            );
        })
    }

    /// Records the geometry's draw with the program, state and bindings into `encoder`; the
    /// caller submits the frame. The frame retains the reservations of the buffers and textures
    /// the draw uses and of the target it draws into, as `PreparedProgramDraw::retain` does, so
    /// their bytes stay counted until the frame is cancelled, dropped or completed on the GPU,
    /// even when the runtime is dropped first.
    ///
    /// # Errors
    /// Fails on an unknown handle, a binding that does not fit the program, an encoder that is
    /// not the authority's, a lost device or a GPU validation error such as a vertex layout
    /// that does not match the shader's inputs.
    pub fn draw(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        request: &DrawRequest<'_>,
    ) -> Result<(), ProgramError> {
        self.recording(encoder)?;
        let target = self.store.target(request.target)?;
        let signature = TargetSignature::new(
            &[target.color.texture.format()],
            target.depth.as_ref().map(|d| d.texture.format()),
            target.color.texture.sample_count(),
        )?;
        let prepared = self.prepare_pinned(&request.packet(), signature, Some(request.target))?;
        prepared.retain(encoder)?;
        let target = self.store.target(request.target)?;
        self.guarded(|| {
            let mut pass = open_pass(encoder, target);
            prepared.record(&mut pass);
        })
    }
}

/// A pass over the target that keeps what it holds.
fn open_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &TargetEntry,
) -> wgpu::RenderPass<'e> {
    let attachment = wgpu::RenderPassColorAttachment {
        view: &target.color.view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    };
    let depth = target
        .depth
        .as_ref()
        .map(|d| wgpu::RenderPassDepthStencilAttachment {
            view: &d.view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        });
    encoder.begin_render_pass(&pass_descriptor(&[Some(attachment)], depth))
}

fn pass_descriptor<'a>(
    colors: &'a [Option<wgpu::RenderPassColorAttachment<'a>>],
    depth: Option<wgpu::RenderPassDepthStencilAttachment<'a>>,
) -> wgpu::RenderPassDescriptor<'a> {
    wgpu::RenderPassDescriptor {
        label: Some("render-program pass"),
        color_attachments: colors,
        depth_stencil_attachment: depth,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }
}
