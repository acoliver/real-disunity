//! A draw prepared against the signature of the pass it will join, and recorded into a pass
//! someone else opened.
//!
//! Preparation does everything that can fail or allocate: it builds the pipeline for the pass's
//! attachment formats and sample count, the bind groups and the buffer references. Recording is
//! then only state setting and a draw call, so it cannot fail halfway through a pass.

use crate::draw::{DrawRequest, Geometry, VertexInput};
use crate::error::{ProgramError, UnsupportedTarget};
use crate::pipeline_key::Key;
use crate::program::GraphicsProgram;
use crate::resources::BoundResource;
use crate::runtime::ProgramRuntime;
use crate::state::RenderState;

/// The attachments of a pass: what a pipeline must agree with to draw in it.
///
/// A signature exists only for what the runtime draws into: one single-sample color attachment
/// and, optionally, a depth attachment without stencil.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TargetSignature {
    color: wgpu::TextureFormat,
    depth: Option<wgpu::TextureFormat>,
    samples: u32,
}

impl TargetSignature {
    /// The signature of a pass with `colors` and `depth` attachments of `samples` per pixel.
    ///
    /// # Errors
    /// Fails by name for multisampling, anything but one color attachment, a depth format
    /// with a stencil aspect, and a depth attachment that has no depth.
    pub fn new(
        colors: &[wgpu::TextureFormat],
        depth: Option<wgpu::TextureFormat>,
        samples: u32,
    ) -> Result<Self, ProgramError> {
        if samples != 1 {
            return Err(UnsupportedTarget::Multisampled { samples }.into());
        }
        let &[color] = colors else {
            return Err(UnsupportedTarget::ColorAttachments {
                count: colors.len(),
            }
            .into());
        };
        if let Some(format) = depth {
            if !format.has_depth_aspect() {
                return Err(UnsupportedTarget::NotDepth { format }.into());
            }
            if format.has_stencil_aspect() {
                return Err(UnsupportedTarget::Stencil { format }.into());
            }
        }
        Ok(Self {
            color,
            depth,
            samples,
        })
    }

    #[must_use]
    pub fn color(&self) -> wgpu::TextureFormat {
        self.color
    }

    #[must_use]
    pub fn depth(&self) -> Option<wgpu::TextureFormat> {
        self.depth
    }

    #[must_use]
    pub fn samples(&self) -> u32 {
        self.samples
    }
}

/// One draw of triangles, without the target it draws into.
#[derive(Clone, Copy)]
pub struct DrawPacket<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub vertex_inputs: &'a [VertexInput],
    pub bindings: &'a [BoundResource],
    pub geometry: Geometry,
}

impl<'a> DrawRequest<'a> {
    pub(crate) fn packet(&self) -> DrawPacket<'a> {
        DrawPacket {
            program: self.program,
            state: self.state,
            front_face: self.front_face,
            vertex_inputs: self.vertex_inputs,
            bindings: self.bindings,
            geometry: self.geometry,
        }
    }
}

enum PreparedGeometry {
    Vertices {
        first: u32,
        count: u32,
    },
    Indexed {
        buffer: wgpu::Buffer,
        format: wgpu::IndexFormat,
        first: u32,
        count: u32,
        base_vertex: i32,
    },
}

/// A draw whose pipeline, bind groups and buffers are ready. Recording it touches no runtime
/// state, so it can be recorded into any compatible pass, any number of times.
///
/// It holds clones of the wgpu objects it draws with, which wgpu keeps alive for as long as a
/// command recorded with them is unfinished or in flight.
pub struct PreparedProgramDraw {
    signature: TargetSignature,
    pipeline: wgpu::RenderPipeline,
    groups: Vec<(u32, wgpu::BindGroup)>,
    vertex_buffers: Vec<wgpu::Buffer>,
    geometry: PreparedGeometry,
}

impl std::fmt::Debug for PreparedProgramDraw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedProgramDraw")
            .field("signature", &self.signature)
            .finish_non_exhaustive()
    }
}

impl PreparedProgramDraw {
    /// The attachments of the passes this draw can be recorded into.
    #[must_use]
    pub fn signature(&self) -> TargetSignature {
        self.signature
    }

    /// Records the draw into `pass`, whose attachments must match [`signature`](Self::signature).
    /// A pass that does not match is a GPU validation error when the frame is finished.
    pub fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        for (set, group) in &self.groups {
            pass.set_bind_group(*set, group, &[]);
        }
        for (slot, buffer) in (0..).zip(&self.vertex_buffers) {
            pass.set_vertex_buffer(slot, buffer.slice(..));
        }
        match &self.geometry {
            PreparedGeometry::Vertices { first, count } => pass.draw(*first..*first + *count, 0..1),
            PreparedGeometry::Indexed {
                buffer,
                format,
                first,
                count,
                base_vertex,
            } => {
                pass.set_index_buffer(buffer.slice(..), *format);
                pass.draw_indexed(*first..*first + *count, *base_vertex, 0..1);
            }
        }
    }
}

impl ProgramRuntime {
    /// Prepares `packet` for passes with the attachments of `signature`.
    ///
    /// # Errors
    /// Fails on an unknown handle, a binding that does not fit the program, a lost device or a
    /// GPU validation error such as a vertex layout that does not match the shader's inputs.
    pub fn prepare_draw(
        &mut self,
        packet: &DrawPacket<'_>,
        signature: TargetSignature,
    ) -> Result<PreparedProgramDraw, ProgramError> {
        self.authority.check_device()?;
        self.store
            .check_geometry(packet.vertex_inputs, packet.geometry)?;
        let pipeline = self.render_pipeline(packet, signature)?;
        let groups = self.bind_groups(&packet.program.gpu, packet.bindings)?;
        let vertex_buffers = packet
            .vertex_inputs
            .iter()
            .map(|input| Ok(self.store.buffer(input.buffer)?.buffer.clone()))
            .collect::<Result<_, ProgramError>>()?;
        let geometry = match packet.geometry {
            Geometry::Vertices { first, count } => PreparedGeometry::Vertices { first, count },
            Geometry::Indexed {
                buffer,
                format,
                first,
                count,
                base_vertex,
            } => PreparedGeometry::Indexed {
                buffer: self.store.buffer(buffer)?.buffer.clone(),
                format,
                first,
                count,
                base_vertex,
            },
        };
        Ok(PreparedProgramDraw {
            signature,
            pipeline,
            groups,
            vertex_buffers,
            geometry,
        })
    }

    fn render_pipeline(
        &mut self,
        packet: &DrawPacket<'_>,
        signature: TargetSignature,
    ) -> Result<wgpu::RenderPipeline, ProgramError> {
        let key = pipeline_key(packet, signature);
        if let Some(pipeline) = self.render_pipelines.get(&key) {
            return Ok(pipeline.clone());
        }
        let modules = (
            self.shader_module(&packet.program.vertex)?,
            self.shader_module(&packet.program.fragment)?,
        );
        let pipeline = self.guarded(|| self.create_render_pipeline(packet, &modules, signature))?;
        self.render_pipelines.insert(key, pipeline.clone());
        Ok(pipeline)
    }

    fn create_render_pipeline(
        &self,
        packet: &DrawPacket<'_>,
        (vertex, fragment): &(wgpu::ShaderModule, wgpu::ShaderModule),
        signature: TargetSignature,
    ) -> wgpu::RenderPipeline {
        let layouts: Vec<Option<wgpu::VertexBufferLayout<'_>>> = packet
            .vertex_inputs
            .iter()
            .map(|input| {
                Some(wgpu::VertexBufferLayout {
                    array_stride: input.stride,
                    step_mode: input.step,
                    attributes: &input.attributes,
                })
            })
            .collect();
        let state = packet.state;
        let targets = [Some(wgpu::ColorTargetState {
            format: signature.color,
            blend: state.blend,
            write_mask: state.color_write,
        })];
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("render-program pipeline"),
                layout: Some(&packet.program.gpu.pipeline),
                vertex: wgpu::VertexState {
                    module: vertex,
                    entry_point: Some(&packet.program.vertex.module().entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &layouts,
                },
                primitive: wgpu::PrimitiveState {
                    front_face: packet.front_face,
                    cull_mode: state.cull,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: signature.depth.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: Some(state.depth_write),
                    depth_compare: Some(state.depth_compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: signature.samples,
                    ..wgpu::MultisampleState::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: fragment,
                    entry_point: Some(&packet.program.fragment.module().entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
    }
}

/// Everything a render pipeline is built from, as a content key.
fn pipeline_key(packet: &DrawPacket<'_>, signature: TargetSignature) -> blake3::Hash {
    let mut key = Key::new("render");
    key.hash(packet.program.vertex.content_key());
    key.hash(packet.program.fragment.content_key());
    key.debug(&packet.program.gpu.layout);
    key.debug(&(packet.state, packet.front_face, signature));
    for input in packet.vertex_inputs {
        key.debug(&(input.stride, input.step, &input.attributes));
    }
    key.finish()
}
