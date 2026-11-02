//! Draws of a graphics program into a render target.

use crate::error::ProgramError;
use crate::pipeline_key::Key;
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

/// Which vertices a draw reads.
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
    /// caller submits the frame.
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
        let pipeline = self.render_pipeline(request)?;
        let groups = self.bind_groups(&request.program.gpu, request.bindings)?;
        let target = self.store.target(request.target)?;
        self.guarded(|| {
            self.record_draw(encoder, (&pipeline, &groups), target, request);
        })
    }

    fn record_draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (pipeline, groups): (&wgpu::RenderPipeline, &[(u32, wgpu::BindGroup)]),
        target: &TargetEntry,
        request: &DrawRequest<'_>,
    ) {
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
        let mut pass = encoder.begin_render_pass(&pass_descriptor(&[Some(attachment)], depth));
        pass.set_pipeline(pipeline);
        for (set, group) in groups {
            pass.set_bind_group(*set, group, &[]);
        }
        for (slot, input) in (0..).zip(request.vertex_inputs) {
            if let Ok(entry) = self.store.buffer(input.buffer) {
                pass.set_vertex_buffer(slot, entry.buffer.slice(..));
            }
        }
        match request.geometry {
            Geometry::Vertices { first, count } => pass.draw(first..first + count, 0..1),
            Geometry::Indexed {
                buffer,
                format,
                first,
                count,
                base_vertex,
            } => {
                if let Ok(entry) = self.store.buffer(buffer) {
                    pass.set_index_buffer(entry.buffer.slice(..), format);
                }
                pass.draw_indexed(first..first + count, base_vertex, 0..1);
            }
        }
    }

    fn render_pipeline(
        &mut self,
        request: &DrawRequest<'_>,
    ) -> Result<wgpu::RenderPipeline, ProgramError> {
        let target = self.store.target(request.target)?;
        let formats = (
            target.color.texture.format(),
            target.depth.as_ref().map(|d| d.texture.format()),
        );
        let key = pipeline_key(request, formats);
        if let Some(pipeline) = self.render_pipelines.get(&key) {
            return Ok(pipeline.clone());
        }
        let modules = (
            self.shader_module(&request.program.vertex)?,
            self.shader_module(&request.program.fragment)?,
        );
        let pipeline = self.guarded(|| self.create_render_pipeline(request, &modules, formats))?;
        self.render_pipelines.insert(key, pipeline.clone());
        Ok(pipeline)
    }

    fn create_render_pipeline(
        &self,
        request: &DrawRequest<'_>,
        (vertex, fragment): &(wgpu::ShaderModule, wgpu::ShaderModule),
        (color_format, depth_format): (wgpu::TextureFormat, Option<wgpu::TextureFormat>),
    ) -> wgpu::RenderPipeline {
        let layouts: Vec<Option<wgpu::VertexBufferLayout<'_>>> = request
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
        let state = request.state;
        let targets = [Some(wgpu::ColorTargetState {
            format: color_format,
            blend: state.blend,
            write_mask: state.color_write,
        })];
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("render-program pipeline"),
                layout: Some(&request.program.gpu.pipeline),
                vertex: wgpu::VertexState {
                    module: vertex,
                    entry_point: Some(&request.program.vertex.module().entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &layouts,
                },
                primitive: wgpu::PrimitiveState {
                    front_face: request.front_face,
                    cull_mode: state.cull,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: Some(state.depth_write),
                    depth_compare: Some(state.depth_compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: fragment,
                    entry_point: Some(&request.program.fragment.module().entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
    }
}

/// Everything a render pipeline is built from, as a content key.
fn pipeline_key(
    request: &DrawRequest<'_>,
    formats: (wgpu::TextureFormat, Option<wgpu::TextureFormat>),
) -> blake3::Hash {
    let mut key = Key::new("render");
    key.hash(request.program.vertex.content_key());
    key.hash(request.program.fragment.content_key());
    key.debug(&request.program.gpu.layout);
    key.debug(&(request.state, request.front_face, formats));
    for input in request.vertex_inputs {
        key.debug(&(input.stride, input.step, &input.attributes));
    }
    key.finish()
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
