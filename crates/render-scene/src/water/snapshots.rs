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
/// The name of the post-water depth in the frame's plan: the frame's depth after the stock water,
/// as positive view-space metres.
pub(crate) const POST_WATER_DEPTH: &str = "post-water depth";
/// The name of the post-water device depth in the frame's plan.
pub(crate) const POST_WATER_DEVICE_DEPTH: &str = "post-water device depth";

pub(crate) struct SnapshotFeature {
    pub snapshots: Snapshots,
    pub frame: FrameUniform,
    pub color_source: Option<wgpu::BindGroup>,
    pub depth_source: Option<wgpu::BindGroup>,
    /// Whether the depth snapshot pass also writes the device depth copy.
    pub device_depth: bool,
    /// Whether the post-water depth pass also writes the post-water device depth copy.
    pub final_device_depth: bool,
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
        pass.set_pipeline(self.pipeline(context.slot));
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

impl SnapshotFeature {
    /// The pipeline that writes the pass of `slot`: the one with the device depth as its second
    /// target when the frame asked for that companion and the bank holds it.
    fn pipeline(&self, slot: PassSlot) -> &wgpu::RenderPipeline {
        let snapshots = &self.snapshots;
        let companion = match slot {
            PassSlot::AfterOpaqueSnapshot => return &snapshots.color_pipeline,
            PassSlot::AfterOpaqueDepth => self.device_depth.then_some(&snapshots.device_depth),
            PassSlot::AfterWaterDepth => self
                .final_device_depth
                .then_some(&snapshots.final_device_depth),
            _ => None,
        };
        companion
            .and_then(Option::as_ref)
            .map_or(&snapshots.depth_pipeline, |depth| &depth.pipeline)
    }
}

/// Each admitted slot owns its snapshots. Encoder-ordered writes precede all readers.
#[derive(Clone, Debug)]
pub(crate) struct Snapshots {
    pub color: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub final_depth: wgpu::Texture,
    /// The depth buffer's own values, `R32Float`. Present only when a game reads them, so a frame
    /// without such a game allocates none.
    pub device_depth: Option<DeviceDepth>,
    /// The post-water depth's own values, `R32Float`. Present only when a game reads them.
    pub final_device_depth: Option<DeviceDepth>,
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
    /// These snapshots with the device depth copies asked for, `opaque` for the opaque depth and
    /// `post_water` for the post-water depth. Each is allocated once, and a copy the snapshots
    /// already hold or one not asked for is left as it is. A depth pass then writes its copy beside
    /// its linear depth: the nearest sample of each pixel in the engine's depth convention, where
    /// depth is not reversed, so the nearest is the smallest. Both passes write their copy with
    /// the one pipeline, so the first copy of either kind builds it.
    #[must_use]
    pub fn with_device_depths(
        &self,
        device: &wgpu::Device,
        opaque: bool,
        post_water: bool,
    ) -> Self {
        let missing = |asked: bool, held: &Option<DeviceDepth>| asked && held.is_none();
        if !missing(opaque, &self.device_depth) && !missing(post_water, &self.final_device_depth) {
            return self.clone();
        }
        let pipeline = self
            .device_depth
            .iter()
            .chain(&self.final_device_depth)
            .next()
            .map_or_else(
                || {
                    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("opaque snapshot"),
                        source: wgpu::ShaderSource::Wgsl(snapshot_source(self.samples).into()),
                    });
                    device_depth_pipeline(device, &module, &self.depth_pipeline)
                },
                |held| held.pipeline.clone(),
            );
        let copy = |asked: bool, held: &Option<DeviceDepth>| {
            if missing(asked, held) {
                Some(DeviceDepth {
                    texture: crate::ssao::passes::texture(
                        device,
                        self.size,
                        wgpu::TextureFormat::R32Float,
                    ),
                    pipeline: pipeline.clone(),
                })
            } else {
                held.clone()
            }
        };
        Self {
            device_depth: copy(opaque, &self.device_depth),
            final_device_depth: copy(post_water, &self.final_device_depth),
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
            final_device_depth: None,
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
        // Each snapshot pass: where it runs, what it writes and from what, and the device depth
        // copy it also writes when the frame asked for one.
        let passes_to_plan = [
            (
                (PassSlot::AfterOpaqueSnapshot, request.color),
                (&self.color, "opaque snapshot color", color),
                None,
            ),
            (
                (PassSlot::AfterOpaqueDepth, request.depth),
                (&self.depth, LINEAR_DEPTH, depth),
                request
                    .device
                    .opaque
                    .then_some((DEVICE_DEPTH, &self.device_depth)),
            ),
            (
                (PassSlot::AfterWaterDepth, request.final_depth),
                (&self.final_depth, POST_WATER_DEPTH, depth),
                request
                    .device
                    .post_water
                    .then_some((POST_WATER_DEVICE_DEPTH, &self.final_device_depth)),
            ),
        ];
        for ((slot, requested), (texture, name, source), companion) in passes_to_plan {
            if !requested {
                continue;
            }
            let resource = ResourceRef::new(name);
            let view = match name {
                LINEAR_DEPTH => FrameProduct::opaque_depth(self.samples).attachment(texture)?,
                POST_WATER_DEPTH => FrameProduct::final_depth(self.samples).attachment(texture)?,
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
            if let Some((name, held)) = companion {
                Self::write_device_depth(&mut pass, &mut feature, views, (name, held))?;
            }
            passes.push(pass);
            declaration.passes.push(feature);
        }
        Ok(declaration)
    }

    /// Adds a device depth copy to a depth snapshot pass as its second colour attachment, named
    /// `name` in the frame's plan.
    fn write_device_depth(
        pass: &mut PassDeclaration,
        feature: &mut worldrunner_render_post::FeaturePass,
        views: &mut Vec<AttachmentView>,
        copy: (&'static str, &Option<DeviceDepth>),
    ) -> Result<(), FrameError> {
        let (name, held) = copy;
        let device_depth = held.as_ref().ok_or(FrameError::UnsupportedEffect(
            "the device depth snapshot is not allocated",
        ))?;
        let resource = ResourceRef::new(name);
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
