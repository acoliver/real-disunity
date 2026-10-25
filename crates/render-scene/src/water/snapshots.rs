use crate::frame::FrameUniform;
use worldrunner_render_post::{DrawContext, PrepareContext, PreparedFeature};

pub(crate) struct SnapshotFeature {
    pub snapshots: Snapshots,
    pub frame: FrameUniform,
    pub color_source: Option<wgpu::BindGroup>,
    pub depth_source: Option<wgpu::BindGroup>,
}
impl PreparedFeature for SnapshotFeature {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut PrepareContext<'_>) {
        let bytes: Vec<_> = [
            self.frame.view_projection,
            self.frame.view,
            self.frame.view_projection.inverse(),
        ]
        .into_iter()
        .flat_map(|matrix| matrix.to_cols_array())
        .flat_map(f32::to_le_bytes)
        .collect();
        context.upload_buffer(&self.snapshots.uniform, 0, &bytes);
    }
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(if context.slot == PassSlot::AfterOpaqueSnapshot {
            &self.snapshots.color_pipeline
        } else {
            &self.snapshots.depth_pipeline
        });
        let source = if context.slot == PassSlot::AfterOpaqueSnapshot {
            &self.color_source
        } else {
            &self.depth_source
        };
        pass.set_bind_group(0, source.as_ref().expect("scheduled snapshot input"), &[]);
        pass.draw(0..3, 0..1);
    }
}

use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FrameConfig, FrameError, PassDeclaration, PassSlot,
    ResourceRef,
};

/// Each admitted slot owns its snapshots. Encoder-ordered writes precede all readers.
#[derive(Clone, Debug)]
pub(crate) struct Snapshots {
    pub color: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub final_depth: wgpu::Texture,
    pub uniform: wgpu::Buffer,
    pub size: [u32; 2],
    pub samples: u32,
    pub color_pipeline: wgpu::RenderPipeline,
    pub depth_pipeline: wgpu::RenderPipeline,
}
impl Snapshots {
    pub fn new(device: &wgpu::Device, config: FrameConfig) -> Self {
        let source = snapshot_source(config.samples);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opaque snapshot"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let (color_pipeline, depth_pipeline) = pipelines(device, &module, config.samples);
        Self {
            color: crate::ssao::passes::texture(
                device,
                config.size,
                wgpu::TextureFormat::Rgba16Float,
            ),
            depth: crate::ssao::passes::texture(device, config.size, wgpu::TextureFormat::R32Float),
            final_depth: crate::ssao::passes::texture(
                device,
                config.size,
                wgpu::TextureFormat::R32Float,
            ),
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("water frame"),
                size: 304,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            size: config.size,
            samples: config.samples,
            color_pipeline,
            depth_pipeline,
        }
    }
    pub fn source_group(
        &self,
        device: &wgpu::Device,
        source: &AttachmentView,
        depth: bool,
    ) -> wgpu::BindGroup {
        let pipeline = if depth {
            &self.depth_pipeline
        } else {
            &self.color_pipeline
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opaque snapshot sources"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: if depth { 2 } else { 1 },
                    resource: wgpu::BindingResource::TextureView(source.view()),
                },
            ],
        })
    }
    pub fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
        request: super::planning::SnapshotRequest,
        color: Option<&AttachmentView>,
        depth: Option<&AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let mut declaration = FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        };
        for (slot, texture, name, requested, source) in [
            (
                PassSlot::AfterOpaqueSnapshot,
                &self.color,
                "opaque snapshot color",
                request.color,
                color,
            ),
            (
                PassSlot::AfterOpaqueDepth,
                &self.depth,
                "opaque snapshot depth",
                request.depth,
                depth,
            ),
            (
                PassSlot::AfterWaterDepth,
                &self.final_depth,
                "post-water depth",
                request.final_depth,
                depth,
            ),
        ] {
            if !requested {
                continue;
            }
            let resource = ResourceRef::new(name);
            let view = AttachmentView::new(resource, texture)?;
            let mut pass = PassDeclaration::new(slot);
            pass.color = Some(crate::compositor::attachment(resource, &view, false));
            passes.push(pass);
            views.push(view);
            let mut feature = FeatureDeclaration::new(slot).passes.remove(0);
            feature.color_format = Some(texture.format());
            feature
                .reads
                .push(source.expect("requested snapshot source").resource());
            declaration.passes.push(feature);
        }
        Ok(declaration)
    }
}
fn entry(binding: u32, depth: bool, samples: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: if depth {
                wgpu::TextureSampleType::Depth
            } else {
                wgpu::TextureSampleType::Float { filterable: false }
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: samples > 1,
        },
        count: None,
    }
}

fn pipelines(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    samples: u32,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let pipeline = |format, entry, depth| {
        let layout = source_layout(device, samples, depth);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("snapshots"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("after opaque snapshot"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_snapshot"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };

    (
        pipeline(wgpu::TextureFormat::Rgba16Float, "fs_color", false),
        pipeline(wgpu::TextureFormat::R32Float, "fs_depth", true),
    )
}

fn source_layout(device: &wgpu::Device, samples: u32, depth: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snapshot source"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            entry(if depth { 2 } else { 1 }, depth, samples),
        ],
    })
}

fn snapshot_source(samples: u32) -> String {
    include_str!("snapshot.wgsl")
        .replace(
            "SOURCE_COLOR",
            if samples == 1 {
                "texture_2d<f32>"
            } else {
                "texture_multisampled_2d<f32>"
            },
        )
        .replace(
            "SOURCE_DEPTH",
            if samples == 1 {
                "texture_depth_2d"
            } else {
                "texture_depth_multisampled_2d"
            },
        )
        .replace("SAMPLE_COUNT", &samples.to_string())
        .replace(
            "DEPTH_LOAD",
            if samples == 1 {
                "textureLoad(source_depth, pixel, 0)"
            } else {
                "textureLoad(source_depth, pixel, sample_index)"
            },
        )
        .replace(
            "COLOR_LOAD",
            if samples == 1 {
                "textureLoad(source_color, pixel, 0)"
            } else {
                "textureLoad(source_color, pixel, sample_index)"
            },
        )
}
