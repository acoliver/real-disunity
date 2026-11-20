use crate::frame::FrameUniform;
use crate::frame::metadata::FrameProduct;
use crate::shader::rewrite::ShaderRewrite;
use worldrunner_render_post::{DrawContext, PrepareContext, PreparedFeature};

/// The name of the opaque colour snapshot in the frame's plan.
pub(crate) const COLOR: &str = "opaque snapshot color";
/// The name of the linear view-space depth snapshot in the frame's plan.
pub(crate) const LINEAR_DEPTH: &str = "opaque snapshot depth";
/// The name of the device depth snapshot in the frame's plan.
pub(crate) const DEVICE_DEPTH: &str = "opaque snapshot device depth";

pub(crate) struct SnapshotFeature {
    pub snapshots: Snapshots,
    pub frame: FrameUniform,
    pub color_source: Option<wgpu::BindGroup>,
    pub depth_source: Option<wgpu::BindGroup>,
    /// Whether the depth snapshot pass also writes the device depth copy.
    pub device_depth: bool,
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
        pass.set_pipeline(match (context.slot, &self.snapshots.device_depth) {
            (PassSlot::AfterOpaqueSnapshot, _) => &self.snapshots.color_pipeline,
            (PassSlot::AfterOpaqueDepth, Some(device_depth)) if self.device_depth => {
                &device_depth.pipeline
            }
            _ => &self.snapshots.depth_pipeline,
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
    /// The depth buffer's own values, `R32Float`. Present only when a game reads them, so a frame
    /// without such a game allocates none.
    pub device_depth: Option<DeviceDepth>,
    pub uniform: wgpu::Buffer,
    pub size: [u32; 2],
    pub samples: u32,
    pub color_pipeline: wgpu::RenderPipeline,
    pub depth_pipeline: wgpu::RenderPipeline,
}
/// The device depth copy of one frame slot, and the pass that writes it beside the linear depth.
#[derive(Clone, Debug)]
pub(crate) struct DeviceDepth {
    pub texture: wgpu::Texture,
    pub pipeline: wgpu::RenderPipeline,
}

impl Snapshots {
    /// These snapshots with the device depth copy, which allocates it once. The linear depth pass
    /// then writes both, the nearest sample of each pixel in the engine's depth convention: depth
    /// is not reversed, so the nearest is the smallest.
    #[must_use]
    pub fn with_device_depth(&self, device: &wgpu::Device) -> Self {
        if self.device_depth.is_some() {
            return self.clone();
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opaque snapshot"),
            source: wgpu::ShaderSource::Wgsl(snapshot_source(self.samples).into()),
        });
        Self {
            device_depth: Some(DeviceDepth {
                texture: crate::ssao::passes::texture(
                    device,
                    self.size,
                    wgpu::TextureFormat::R32Float,
                ),
                pipeline: device_depth_pipeline(device, &module, &self.depth_pipeline),
            }),
            ..self.clone()
        }
    }

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
            device_depth: None,
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
        let depth_view = depth.then(|| {
            source.texture().create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::DepthOnly,
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_mip_level: source.resource().mip,
                mip_level_count: Some(1),
                base_array_layer: source.resource().layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
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
                    resource: wgpu::BindingResource::TextureView(
                        depth_view.as_ref().unwrap_or_else(|| source.view()),
                    ),
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
            let view = match name {
                "opaque snapshot depth" => {
                    FrameProduct::opaque_depth(self.samples).attachment(texture)?
                }
                "post-water depth" => {
                    FrameProduct::final_depth(self.samples).attachment(texture)?
                }
                _ => AttachmentView::new(resource, texture)?,
            };
            let mut pass = PassDeclaration::new(slot);
            pass.color = Some(crate::compositor::attachment(resource, &view, false));
            views.push(view);
            let mut feature = FeatureDeclaration::new(slot).passes.remove(0);
            feature.color_format = Some(texture.format());
            feature
                .reads
                .push(source.expect("requested snapshot source").resource());
            if slot == PassSlot::AfterOpaqueDepth && request.device_depth {
                self.write_device_depth(&mut pass, &mut feature, views)?;
            }
            passes.push(pass);
            declaration.passes.push(feature);
        }
        Ok(declaration)
    }

    /// Adds the device depth copy to the depth snapshot pass as its second colour attachment.
    fn write_device_depth(
        &self,
        pass: &mut PassDeclaration,
        feature: &mut worldrunner_render_post::FeaturePass,
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        let device_depth = self
            .device_depth
            .as_ref()
            .ok_or(FrameError::UnsupportedEffect(
                "the device depth snapshot is not allocated",
            ))?;
        let resource = ResourceRef::new(DEVICE_DEPTH);
        let view = AttachmentView::new(resource, &device_depth.texture)?;
        pass.additional_colors
            .push(crate::compositor::attachment(resource, &view, false));
        feature
            .additional_color_formats
            .push(device_depth.texture.format());
        views.push(view);
        Ok(())
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

/// The depth snapshot pass that also writes the device depth, as its second colour attachment. It
/// shares the bind group layout of `linear`, so one source group serves both pipelines.
fn device_depth_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    linear: &wgpu::RenderPipeline,
) -> wgpu::RenderPipeline {
    let layout = linear.get_bind_group_layout(0);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("snapshots with device depth"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let target = Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::R32Float,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("after opaque depth snapshot with device depth"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_snapshot"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_depth_device"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[target.clone(), target],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
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
        .anchored(
            "water::snapshots::snapshot_source",
            1,
            "SOURCE_COLOR",
            if samples == 1 {
                "texture_2d<f32>"
            } else {
                "texture_multisampled_2d<f32>"
            },
        )
        .anchored(
            "water::snapshots::snapshot_source",
            1,
            "SOURCE_DEPTH",
            if samples == 1 {
                "texture_depth_2d"
            } else {
                "texture_depth_multisampled_2d"
            },
        )
        .anchored(
            "water::snapshots::snapshot_source",
            3,
            "SAMPLE_COUNT",
            &samples.to_string(),
        )
        .anchored(
            "water::snapshots::snapshot_source",
            1,
            "DEPTH_LOAD",
            if samples == 1 {
                "textureLoad(source_depth, pixel, 0)"
            } else {
                "textureLoad(source_depth, pixel, sample_index)"
            },
        )
        .anchored(
            "water::snapshots::snapshot_source",
            1,
            "COLOR_LOAD",
            if samples == 1 {
                "textureLoad(source_color, pixel, 0)"
            } else {
                "textureLoad(source_color, pixel, sample_index)"
            },
        )
}
