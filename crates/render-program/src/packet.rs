//! A draw prepared against the signature of the pass it will join, and recorded into a pass
//! someone else opened.
//!
//! Preparation does everything that can fail or allocate: it builds the pipeline for the pass's
//! attachment formats and sample count, the bind groups and the buffer references. Recording is
//! then only state setting and a draw call, so it cannot fail halfway through a pass.

use std::sync::Arc;

use crate::budget::Reservation;
use crate::draw::{DrawRequest, Geometry, VertexInput};
use crate::error::{ProgramError, UnsupportedTarget};
use crate::pipeline_key::Key;
use crate::program::GraphicsProgram;
use crate::resources::{BoundResource, Pins, Resource};
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

#[derive(Debug)]
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

/// A draw whose pipeline, bind groups and buffers are ready, without the hold on the budget.
/// Recording it touches no runtime state.
#[derive(Debug)]
pub(crate) struct RecordableDraw {
    signature: TargetSignature,
    pipeline: wgpu::RenderPipeline,
    groups: Vec<(u32, wgpu::BindGroup)>,
    vertex_buffers: Vec<wgpu::Buffer>,
    geometry: PreparedGeometry,
}

impl RecordableDraw {
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
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

/// A draw whose pipeline, bind groups and buffers are ready. Recording it touches no runtime
/// state, so it can be recorded into any compatible pass, any number of times.
///
/// It holds clones of the wgpu objects it draws with, which wgpu keeps alive for as long as a
/// command recorded with them is unfinished or in flight, and shares of the runtime's
/// reservations of the resources they belong to. Those bytes stay counted when the resources are
/// released, for as long as the draw is held and then for every frame that [`retain`](Self::retain)ed
/// the draw, until that frame is cancelled or the GPU has completed it. A frame that records the
/// draw without retaining it leaves nothing counted for it once the draw is dropped.
pub struct PreparedProgramDraw {
    pins: Pins,
    draw: RecordableDraw,
}

impl std::fmt::Debug for PreparedProgramDraw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedProgramDraw")
            .field("signature", &self.draw.signature)
            .finish_non_exhaustive()
    }
}

impl PreparedProgramDraw {
    /// The attachments of the passes this draw can be recorded into.
    #[must_use]
    pub fn signature(&self) -> TargetSignature {
        self.draw.signature
    }

    /// Makes the frame `encoder` records hold the draw's resource reservations until the frame
    /// is cancelled, dropped or completed on the GPU, however long the draw and the runtime
    /// live. Call it for every frame the draw is recorded into.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority
    /// than the runtime's and when `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.pins.retain(encoder)
    }

    /// Records the draw into `pass`, whose attachments must match [`signature`](Self::signature).
    /// A pass that does not match is a GPU validation error when the frame is finished.
    pub fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw.record(pass);
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
        let (draw, shares) = self.prepare_recordable(packet, signature)?;
        Ok(PreparedProgramDraw {
            pins: Pins::new(&self.authority, shares),
            draw,
        })
    }

    /// The draw and the reservation shares of the resources it uses, for the caller to pin.
    pub(crate) fn prepare_recordable(
        &mut self,
        packet: &DrawPacket<'_>,
        signature: TargetSignature,
    ) -> Result<(RecordableDraw, Vec<Arc<Reservation>>), ProgramError> {
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
        let draw = RecordableDraw {
            signature,
            pipeline,
            groups,
            vertex_buffers,
            geometry,
        };
        Ok((draw, self.reservation_shares(packet)?))
    }

    /// Shares of the reservations of every buffer and texture the packet draws with.
    fn reservation_shares(
        &self,
        packet: &DrawPacket<'_>,
    ) -> Result<Vec<Arc<Reservation>>, ProgramError> {
        let mut reservations = Vec::new();
        for bound in packet.bindings {
            reservations.push(match bound.resource {
                Resource::Buffer(buffer) => self.store.buffer_reservation(buffer)?,
                Resource::Texture(texture) => self.store.texture_reservation(texture)?,
            });
        }
        for input in packet.vertex_inputs {
            reservations.push(self.store.buffer_reservation(input.buffer)?);
        }
        if let Geometry::Indexed { buffer, .. } = packet.geometry {
            reservations.push(self.store.buffer_reservation(buffer)?);
        }
        Ok(reservations)
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
        let modules = self.program_modules(packet.program)?;
        let pipeline = self.guarded(|| self.create_render_pipeline(packet, &modules, signature))?;
        self.render_pipelines.insert(key, pipeline.clone());
        Ok(pipeline)
    }

    /// The compiled vertex and fragment modules of a program.
    pub(crate) fn program_modules(
        &mut self,
        program: &GraphicsProgram,
    ) -> Result<(wgpu::ShaderModule, wgpu::ShaderModule), ProgramError> {
        Ok((
            self.shader_module(&program.vertex)?,
            self.shader_module(&program.fragment)?,
        ))
    }

    fn create_render_pipeline(
        &self,
        packet: &DrawPacket<'_>,
        modules: &(wgpu::ShaderModule, wgpu::ShaderModule),
        signature: TargetSignature,
    ) -> wgpu::RenderPipeline {
        with_pipeline_descriptor(
            packet,
            modules,
            signature,
            &packet.program.gpu.pipeline,
            |descriptor| self.device.create_render_pipeline(descriptor),
        )
    }
}

/// Builds the descriptor of the pipeline `packet` draws with against `layout` and hands it to
/// `create`. The checked operations create their pipelines from the same descriptor, with a
/// layout made of their own bind groups.
pub(crate) fn with_pipeline_descriptor<T>(
    packet: &DrawPacket<'_>,
    (vertex, fragment): &(wgpu::ShaderModule, wgpu::ShaderModule),
    signature: TargetSignature,
    layout: &wgpu::PipelineLayout,
    create: impl FnOnce(&wgpu::RenderPipelineDescriptor<'_>) -> T,
) -> T {
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
    create(&wgpu::RenderPipelineDescriptor {
        label: Some("render-program pipeline"),
        layout: Some(layout),
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

/// Everything a render pipeline is built from, as a content key.
pub(crate) fn pipeline_key(packet: &DrawPacket<'_>, signature: TargetSignature) -> blake3::Hash {
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
