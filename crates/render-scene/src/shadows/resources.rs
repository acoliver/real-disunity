use crate::geometry::{instance_attributes, vertex_attributes};
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::shader::ENVIRONMENT_WGSL;
use crate::upload::upload_buffer;
use worldrunner_render_shadow::{
    DepthFragmentState, DepthMap, DepthMapBudget, DepthMapDescriptor, DepthMapError,
    DepthPipelineDescriptor, ReceiverDescriptor, ReceiverDimension,
};

use super::DirectionalShadowSettings;

const SHADOW_UNIFORM_BYTES: usize = 336;

pub(super) const SHADOWED_WGSL: &str = concat!(
    worldrunner_render_shadow::shadow_wgsl!(),
    worldrunner_render_lighting::lighting_wgsl!(),
    worldrunner_render_post::post_wgsl!(),
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/environment.wgsl")
);

pub(super) const SHADOWED_TERRAIN_WGSL: &str = concat!(
    worldrunner_render_shadow::shadow_wgsl!(),
    worldrunner_render_lighting::lighting_wgsl!(),
    worldrunner_render_post::post_wgsl!(),
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/terrain.wgsl")
);

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    worldrunner_render_shadow::receiver_layout(
        device,
        ReceiverDescriptor {
            label: Some("directional shadow"),
            dimension: ReceiverDimension::D2Array,
            texture_binding: 1,
            sampler_binding: 2,
            uniform_binding: 0,
            min_uniform_binding_size: None,
        },
    )
}

pub(super) fn depth_pipelines(
    device: &wgpu::Device,
    pipelines: &ScenePipelines,
    settings: DirectionalShadowSettings,
) -> ([wgpu::RenderPipeline; 4], wgpu::RenderPipeline) {
    let configuration = settings.configuration();
    let module = depth_shader(device);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow caster"),
        bind_group_layouts: &[
            Some(pipelines.frame_layout()),
            Some(pipelines.material_layout()),
        ],
        immediate_size: 0,
    });
    let vertices = vertex_attributes();
    let instances = instance_attributes();
    let buffers = crate::renderer::vertex_buffers(&vertices, &instances);
    let terrain = terrain_depth_pipeline(
        device,
        &module,
        pipelines.frame_layout(),
        &buffers,
        settings,
    );
    let objects = PipelineKey::ALL.map(|key| {
        worldrunner_render_shadow::create_depth_pipeline(
            device,
            DepthPipelineDescriptor {
                label: Some("environment"),
                layout: &layout,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(crate::shader::VERTEX_ENTRY_POINT),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: key.cull_mode(),
                    ..Default::default()
                },
                compare: configuration.caster_compare,
                bias: configuration.caster_bias,
                fragment: Some(DepthFragmentState {
                    module: &module,
                    entry_point: Some(if key.masked {
                        "fs_depth_masked"
                    } else {
                        "fs_depth_opaque"
                    }),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
            },
        )
    });
    (objects, terrain)
}

fn terrain_depth_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    frame_layout: &wgpu::BindGroupLayout,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    settings: DirectionalShadowSettings,
) -> wgpu::RenderPipeline {
    let configuration = settings.configuration();
    let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain shadow caster"),
        bind_group_layouts: &[Some(frame_layout)],
        immediate_size: 0,
    });
    worldrunner_render_shadow::create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: Some("terrain"),
            layout: &terrain_layout,
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(crate::shader::VERTEX_ENTRY_POINT),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            compare: configuration.caster_compare,
            bias: configuration.caster_bias,
            fragment: None,
        },
    )
}

fn depth_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    let source = format!(
        "{ENVIRONMENT_WGSL}\n{}",
        include_str!("../shader/shadow_depth.wgsl")
    );
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow caster"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

#[derive(Debug)]
pub(super) struct ShadowMap {
    pub depth: DepthMap,
    pub uniform: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    slots: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
}

impl ShadowMap {
    pub fn select_frame_slot(&mut self, slot: usize) {
        self.uniform = self.slots[slot].0.clone();
        self.group = self.slots[slot].1.clone();
    }
    pub fn new(
        context: &crate::device::GpuContext,
        settings: DirectionalShadowSettings,
        layout: &wgpu::BindGroupLayout,
        layers: u32,
    ) -> Result<Self, DepthMapError> {
        let device = context.device();
        let max_resolution = u64::from(super::MAX_SHADOW_RESOLUTION);
        let depth = DepthMap::new(
            device,
            DepthMapDescriptor {
                label: Some("directional shadow depth"),
                resolution: settings.resolution,
                layers,
                receiver_dimension: ReceiverDimension::D2Array,
                readback: false,
                budget: DepthMapBudget {
                    max_resolution: super::MAX_SHADOW_RESOLUTION,
                    max_layers: 4,
                    max_texture_bytes: max_resolution * max_resolution * 4 * 4,
                    max_readback_bytes: 0,
                },
            },
        )?;
        let sampler = settings.configuration().comparison_sampler(device);
        let slots: Vec<_> = (0..3)
            .map(|_| {
                let uniform = upload_buffer(
                    device,
                    context.queue(),
                    "shadow uniform",
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
                    &[0; SHADOW_UNIFORM_BYTES],
                );
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("directional shadow"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(depth.receiver_view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                (uniform, group)
            })
            .collect();
        Ok(Self {
            depth,
            uniform: slots[0].0.clone(),
            group: slots[0].1.clone(),
            slots,
        })
    }
}

pub(super) fn uniform_bytes(
    cascades: &super::cascades::CascadeViews,
    settings: DirectionalShadowSettings,
) -> Vec<u8> {
    let mut values = Vec::with_capacity(SHADOW_UNIFORM_BYTES / 4);
    for index in 0..4 {
        values.extend(
            cascades
                .matrices
                .get(index)
                .copied()
                .unwrap_or(glam::Mat4::IDENTITY)
                .to_cols_array(),
        );
    }
    values.extend([0.0, settings.normal_bias_meters, 1.0, 0.0]);
    values.extend([
        f32::from(u16::try_from(cascades.matrices.len()).expect("four cascades")),
        cascades.blend,
        f32::from(u8::from(cascades.camera_splits)),
        cascades.near,
    ]);
    values.extend(cascades.splits);
    values.extend(cascades.view_depth.to_array());
    values.extend(cascades.depth_bias);
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

pub(super) fn depth_frames(
    context: &crate::device::GpuContext,
    layout: &wgpu::BindGroupLayout,
) -> Vec<(wgpu::Buffer, wgpu::BindGroup)> {
    use crate::frame::FRAME_UNIFORM_BYTES;
    (0..4)
        .map(|_| {
            let buffer = upload_buffer(
                context.device(),
                context.queue(),
                "shadow frame",
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
                &[0; FRAME_UNIFORM_BYTES],
            );
            let group = context
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shadow frame"),
                    layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
            (buffer, group)
        })
        .collect()
}

pub(super) fn receiver_pipelines(
    context: &crate::device::GpuContext,
    scene: &worldrunner_render_plan::RenderScene,
    target: (wgpu::TextureFormat, u32),
    shadow_layout: &wgpu::BindGroupLayout,
) -> Result<ScenePipelines, super::ShadowError> {
    let (format, samples) = target;
    let screen_layout = scene
        .effects()
        .and_then(|effects| effects.ssao())
        .map(|_| crate::ssao::ambient_layout(context.device()));
    if screen_layout.is_some() && format != wgpu::TextureFormat::Rgba16Float {
        return Err(crate::frame::FrameError::UnsupportedEffect("SSAO").into());
    }
    let source = screen_layout.as_ref().map_or_else(
        || SHADOWED_WGSL.to_owned(),
        |_| crate::ssao::ambient_source(SHADOWED_WGSL),
    );
    let terrain_source = screen_layout.as_ref().map_or_else(
        || SHADOWED_TERRAIN_WGSL.to_owned(),
        |_| crate::ssao::ambient_source(SHADOWED_TERRAIN_WGSL),
    );
    let pipelines = ScenePipelines::with_screen_space(
        context.device(),
        (format, samples),
        &source,
        &terrain_source,
        Some(shadow_layout),
        screen_layout.as_ref(),
    );

    Ok(pipelines)
}
