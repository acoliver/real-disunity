//! A draw prepared against the signature of the pass it will join, and recorded into a pass
//! someone else opened.
//!
//! Preparation does everything that can fail or allocate: it builds the pipeline for the pass's
//! attachment formats and sample count, the bind groups and the buffer references. Recording is
//! then only state setting and a draw call, so it cannot fail halfway through a pass.

use std::ops::Range;
use std::sync::Arc;

use crate::bind::Snapshot;
use crate::budget::Reservation;
use crate::draw::{DrawRequest, Geometry, VertexInput};
use crate::error::{ProgramError, UnsupportedTarget};
use crate::pipeline_key::Key;
use crate::program::GraphicsProgram;
use crate::resources::{BoundResource, Pins, Resource, TargetHandle};
use crate::runtime::ProgramRuntime;
use crate::state::{ColorState, DepthState, RenderState};

/// The most colour attachments a signature describes.
pub const MAX_COLOR_ATTACHMENTS: usize = 4;

/// The attachments of a pass: what a pipeline must agree with to draw in it.
///
/// A signature exists only for what the runtime draws into: one to four colour attachments of
/// one sample count (1, 2, 4, 8 or 16) and, optionally, a depth attachment without stencil. A
/// depth attachment with no colour attachment is the one signature [`depth_only`](Self::depth_only)
/// makes.
/// [`new`](Self::new) describes the pass the runtime opens for its own targets, one
/// single-sample colour attachment, and [`attachments`](Self::attachments) the passes a
/// renderer opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TargetSignature {
    colors: [Option<wgpu::TextureFormat>; MAX_COLOR_ATTACHMENTS],
    depth: Option<DepthFormat>,
    samples: u32,
}

/// The depth formats a signature admits, which are the ones without a stencil aspect. Keeping
/// them as a byte holds a signature, and so an error that names two of them, within the size
/// the workspace lints allow a returned error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DepthFormat {
    Depth16Unorm,
    Depth24Plus,
    Depth32Float,
}

impl DepthFormat {
    fn of(format: wgpu::TextureFormat) -> Option<Self> {
        match format {
            wgpu::TextureFormat::Depth16Unorm => Some(Self::Depth16Unorm),
            wgpu::TextureFormat::Depth24Plus => Some(Self::Depth24Plus),
            wgpu::TextureFormat::Depth32Float => Some(Self::Depth32Float),
            _ => None,
        }
    }

    fn format(self) -> wgpu::TextureFormat {
        match self {
            Self::Depth16Unorm => wgpu::TextureFormat::Depth16Unorm,
            Self::Depth24Plus => wgpu::TextureFormat::Depth24Plus,
            Self::Depth32Float => wgpu::TextureFormat::Depth32Float,
        }
    }
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
        if colors.len() != 1 {
            return Err(UnsupportedTarget::ColorAttachments {
                count: colors.len(),
            }
            .into());
        }
        Self::build(colors, depth, samples)
    }

    /// The signature of a single-sample pass with a depth attachment and no colour attachment,
    /// which offscreen depth-only render textures are drawn through.
    ///
    /// # Errors
    /// Fails by name for a depth format with a stencil aspect, and for a format that has no depth.
    pub(crate) fn depth_only(depth: wgpu::TextureFormat) -> Result<Self, ProgramError> {
        Self::build(&[], Some(depth), 1)
    }

    /// The signature of a pass a renderer opens: one to four colour attachments, the first of
    /// which the fragment stage writes and the others of which a draw may leave unwritten with
    /// an empty write mask, and `samples` of 1, 2, 4, 8 or 16 per pixel. A pass with the same attachments
    /// and one sample is equal to [`new`](Self::new)'s.
    ///
    /// # Errors
    /// Fails by name for a count of colour attachments outside one to four, a sample count
    /// that is not a power of two up to 16, a depth format with a stencil aspect, and a depth attachment that has
    /// no depth.
    pub fn attachments(
        colors: &[wgpu::TextureFormat],
        depth: Option<wgpu::TextureFormat>,
        samples: u32,
    ) -> Result<Self, ProgramError> {
        if !matches!(samples, 1 | 2 | 4 | 8 | 16) {
            return Err(UnsupportedTarget::SampleCount { samples }.into());
        }
        if !(1..=MAX_COLOR_ATTACHMENTS).contains(&colors.len()) {
            return Err(UnsupportedTarget::ColorAttachments {
                count: colors.len(),
            }
            .into());
        }
        Self::build(colors, depth, samples)
    }

    fn build(
        colors: &[wgpu::TextureFormat],
        depth: Option<wgpu::TextureFormat>,
        samples: u32,
    ) -> Result<Self, ProgramError> {
        let depth = depth.map(depth_format).transpose()?;
        let mut slots = [None; MAX_COLOR_ATTACHMENTS];
        for (slot, format) in slots.iter_mut().zip(colors) {
            *slot = Some(*format);
        }
        Ok(Self {
            colors: slots,
            depth,
            samples,
        })
    }

    /// The first colour attachment, or `None` for a depth-only pass.
    #[must_use]
    pub fn color(&self) -> Option<wgpu::TextureFormat> {
        self.colors[0]
    }

    /// The colour attachments in order.
    pub fn colors(&self) -> impl Iterator<Item = wgpu::TextureFormat> + '_ {
        self.colors.iter().copied().flatten()
    }

    #[must_use]
    pub fn color_count(&self) -> usize {
        self.colors().count()
    }

    #[must_use]
    pub fn depth(&self) -> Option<wgpu::TextureFormat> {
        self.depth.map(DepthFormat::format)
    }

    #[must_use]
    pub fn samples(&self) -> u32 {
        self.samples
    }
}

fn depth_format(format: wgpu::TextureFormat) -> Result<DepthFormat, ProgramError> {
    if !format.has_depth_aspect() {
        return Err(UnsupportedTarget::NotDepth { format }.into());
    }
    if format.has_stencil_aspect() {
        return Err(UnsupportedTarget::Stencil { format }.into());
    }
    DepthFormat::of(format).ok_or_else(|| UnsupportedTarget::NotDepth { format }.into())
}

/// One draw of triangles, without the target it draws into.
#[derive(Clone, Copy)]
pub struct DrawPacket<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    pub vertex_inputs: &'a [VertexInput],
    pub bindings: &'a [BoundResource],
    pub geometry: Geometry,
}

impl<'a> DrawRequest<'a> {
    pub(crate) fn packet(&self) -> DrawPacket<'a> {
        DrawPacket {
            program: self.program,
            state: self.state,
            vertex_inputs: self.vertex_inputs,
            bindings: self.bindings,
            geometry: self.geometry,
        }
    }
}

/// What a material draw adds to a plain packet: the constants its properties packed and the
/// instances it draws.
pub(crate) struct MaterialParts<'a> {
    pub(crate) snapshots: &'a [Snapshot],
    pub(crate) instances: Range<u32>,
}

#[derive(Debug)]
pub(crate) enum PreparedGeometry {
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
    pub(crate) signature: TargetSignature,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) groups: Vec<(u32, wgpu::BindGroup)>,
    pub(crate) vertex_buffers: Vec<wgpu::Buffer>,
    pub(crate) geometry: PreparedGeometry,
    pub(crate) instances: Range<u32>,
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
            PreparedGeometry::Vertices { first, count } => {
                pass.draw(*first..*first + *count, self.instances.clone());
            }
            PreparedGeometry::Indexed {
                buffer,
                format,
                first,
                count,
                base_vertex,
            } => {
                pass.set_index_buffer(buffer.slice(..), *format);
                pass.draw_indexed(
                    *first..*first + *count,
                    *base_vertex,
                    self.instances.clone(),
                );
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
    pub(crate) pins: Pins,
    pub(crate) draw: RecordableDraw,
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
        self.prepare_pinned(packet, signature, (None, None))
    }

    /// The prepared draw, whose pins also cover `target` when the draw is recorded into it.
    pub(crate) fn prepare_pinned(
        &mut self,
        packet: &DrawPacket<'_>,
        signature: TargetSignature,
        (target, parts): (Option<TargetHandle>, Option<&MaterialParts<'_>>),
    ) -> Result<PreparedProgramDraw, ProgramError> {
        let (draw, mut shares) = self.prepare_recordable(packet, signature, parts)?;
        if let Some(target) = target {
            shares.push(self.store.target_reservation(target)?);
        }
        Ok(PreparedProgramDraw {
            pins: Pins::new(&self.authority, shares),
            draw,
        })
    }

    /// The draw and the reservation shares of the resources it uses, for the caller to pin.
    /// `parts` are what a material draw adds: its constant snapshots and instance range.
    pub(crate) fn prepare_recordable(
        &mut self,
        packet: &DrawPacket<'_>,
        signature: TargetSignature,
        parts: Option<&MaterialParts<'_>>,
    ) -> Result<(RecordableDraw, Vec<Arc<Reservation>>), ProgramError> {
        self.authority.check_device()?;
        check_plain(signature)?;
        let color = packet.state.check_drawable()?;
        let (snapshots, instances) = parts.map_or((&[][..], 0..1), |parts| {
            (parts.snapshots, parts.instances.clone())
        });
        self.store.check_instanced_geometry(
            packet.vertex_inputs,
            packet.geometry,
            instance_end(&instances),
        )?;
        let layouts: Vec<_> = packet
            .vertex_inputs
            .iter()
            .map(|input| VertexLayout {
                stride: input.stride,
                step: input.step,
                attributes: &input.attributes,
            })
            .collect();
        let pipeline = self.render_pipeline(&PipelineRequest {
            program: packet.program,
            state: packet.state,
            layouts: &layouts,
            signature,
            colors: &[color],
        })?;
        let groups = self.bind_groups_with(&packet.program.gpu, packet.bindings, snapshots)?;
        let vertex_buffers = packet
            .vertex_inputs
            .iter()
            .map(|input| Ok(self.store.buffer(input.buffer)?.buffer.clone()))
            .collect::<Result<_, ProgramError>>()?;
        let geometry = self.prepared_geometry(packet.geometry)?;
        let draw = RecordableDraw {
            signature,
            pipeline,
            groups,
            vertex_buffers,
            geometry,
            instances,
        };
        let mut shares = self.reservation_shares(packet)?;
        shares.extend(
            snapshots
                .iter()
                .map(|snapshot| snapshot.reservation.clone()),
        );
        Ok((draw, shares))
    }

    fn prepared_geometry(&self, geometry: Geometry) -> Result<PreparedGeometry, ProgramError> {
        Ok(match geometry {
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
        })
    }

    /// Shares of the reservations of every buffer and texture the packet draws with.
    fn reservation_shares(
        &self,
        packet: &DrawPacket<'_>,
    ) -> Result<Vec<Arc<Reservation>>, ProgramError> {
        let mut reservations = self.binding_shares(packet.bindings)?;
        for input in packet.vertex_inputs {
            reservations.push(self.store.buffer_reservation(input.buffer)?);
        }
        if let Geometry::Indexed { buffer, .. } = packet.geometry {
            reservations.push(self.store.buffer_reservation(buffer)?);
        }
        Ok(reservations)
    }

    /// Shares of the reservations of every buffer and texture bound.
    pub(crate) fn binding_shares(
        &self,
        bindings: &[BoundResource],
    ) -> Result<Vec<Arc<Reservation>>, ProgramError> {
        bindings
            .iter()
            .map(|bound| match bound.resource {
                Resource::Buffer(buffer) => self.store.buffer_reservation(buffer),
                Resource::Texture(texture) => self.store.texture_reservation(texture),
            })
            .collect()
    }

    /// The pipeline for `request`, built on first use and cached by content.
    pub(crate) fn render_pipeline(
        &mut self,
        request: &PipelineRequest<'_>,
    ) -> Result<wgpu::RenderPipeline, ProgramError> {
        let key = request.key();
        if let Some(pipeline) = self.render_pipelines.get(&key) {
            return Ok(pipeline.clone());
        }
        let modules = self.program_modules(request.program)?;
        let pipeline = self.guarded(|| {
            with_pipeline_descriptor(
                request,
                &modules,
                &request.program.gpu.pipeline,
                |descriptor| self.device.create_render_pipeline(descriptor),
            )
        })?;
        self.render_pipelines.insert(key, pipeline.clone());
        Ok(pipeline)
    }

    /// The shader modules of a program's two stages, built on first use and cached by content.
    pub(crate) fn program_modules(
        &mut self,
        program: &GraphicsProgram,
    ) -> Result<(wgpu::ShaderModule, wgpu::ShaderModule), ProgramError> {
        Ok((
            self.shader_module(&program.vertex)?,
            self.shader_module(&program.fragment)?,
        ))
    }
}

/// Builds the descriptor of the pipeline `request` describes against `layout` and hands it to
/// `create`. The checked operations create their pipelines from the same descriptor, with a
/// layout made of their own bind groups.
pub(crate) fn with_pipeline_descriptor<T>(
    request: &PipelineRequest<'_>,
    (vertex, fragment): &(wgpu::ShaderModule, wgpu::ShaderModule),
    layout: &wgpu::PipelineLayout,
    create: impl FnOnce(&wgpu::RenderPipelineDescriptor<'_>) -> T,
) -> T {
    let layouts = request.vertex_buffers();
    let targets = request.color_targets();
    let (state, signature) = (request.state, request.signature);
    let depth = state.depth.unwrap_or(DepthState {
        test: wgpu::CompareFunction::Always,
        write: false,
        bias: wgpu::DepthBiasState::default(),
    });
    let program = request.program;
    create(&wgpu::RenderPipelineDescriptor {
        label: Some("render-program pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: vertex,
            entry_point: Some(&program.vertex.module().entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &layouts,
        },
        primitive: wgpu::PrimitiveState {
            front_face: state.raster.front_face,
            cull_mode: state.raster.cull,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: signature.depth().map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(depth.write),
            depth_compare: Some(depth.test),
            stencil: wgpu::StencilState::default(),
            bias: depth.bias,
        }),
        multisample: wgpu::MultisampleState {
            count: signature.samples,
            ..wgpu::MultisampleState::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: fragment,
            entry_point: Some(&program.fragment.module().entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// How a draw reads one vertex buffer.
#[derive(Clone, Copy)]
pub(crate) struct VertexLayout<'a> {
    pub(crate) stride: u64,
    pub(crate) step: wgpu::VertexStepMode,
    pub(crate) attributes: &'a [wgpu::VertexAttribute],
}

/// Everything a render pipeline is built from. `colors` has one state per colour attachment of
/// the signature.
pub(crate) struct PipelineRequest<'a> {
    pub(crate) program: &'a GraphicsProgram,
    pub(crate) state: &'a RenderState,
    pub(crate) layouts: &'a [VertexLayout<'a>],
    pub(crate) signature: TargetSignature,
    pub(crate) colors: &'a [ColorState],
}

impl PipelineRequest<'_> {
    fn vertex_buffers(&self) -> Vec<Option<wgpu::VertexBufferLayout<'_>>> {
        self.layouts
            .iter()
            .map(|layout| {
                Some(wgpu::VertexBufferLayout {
                    array_stride: layout.stride,
                    step_mode: layout.step,
                    attributes: layout.attributes,
                })
            })
            .collect()
    }

    fn color_targets(&self) -> Vec<Option<wgpu::ColorTargetState>> {
        self.signature
            .colors()
            .zip(self.colors)
            .map(|(format, color)| {
                Some(wgpu::ColorTargetState {
                    format,
                    blend: color.blend,
                    write_mask: color.write_mask,
                })
            })
            .collect()
    }

    /// The pipeline's content key: stage sources, layout, the whole state, the target signature
    /// and the vertex layouts.
    pub(crate) fn key(&self) -> blake3::Hash {
        let mut key = Key::new("render");
        key.hash(self.program.vertex.content_key());
        key.hash(self.program.fragment.content_key());
        key.debug(&self.program.gpu.layout);
        key.debug(&(self.state, self.signature));
        for layout in self.layouts {
            key.debug(&(layout.stride, layout.step, layout.attributes));
        }
        key.finish()
    }
}

/// The plain and material draws record into one single-sample colour attachment.
fn check_plain(signature: TargetSignature) -> Result<(), ProgramError> {
    if signature.samples() != 1 {
        return Err(UnsupportedTarget::Multisampled {
            samples: signature.samples(),
        }
        .into());
    }
    if signature.color_count() != 1 {
        return Err(UnsupportedTarget::ColorAttachments {
            count: signature.color_count(),
        }
        .into());
    }
    Ok(())
}

/// The instance strides an instance-rate vertex buffer must hold for `instances`.
pub(crate) fn instance_end(instances: &Range<u32>) -> u32 {
    if instances.is_empty() {
        0
    } else {
        instances.end
    }
}
