use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::shader::ENVIRONMENT_WGSL;
use crate::upload::upload_buffer;
use worldrunner_render_shadow::{
    DepthFragmentState, DepthMap, DepthMapBudget, DepthMapDescriptor, DepthMapError,
    DepthPipelineDescriptor, ReceiverDescriptor, ReceiverDimension,
};

use super::DirectionalShadowSettings;

const SHADOW_UNIFORM_BYTES: usize = 80;

pub(super) const SHADOWED_WGSL: &str = concat!(
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/environment.wgsl")
);

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    worldrunner_render_shadow::receiver_layout(
        device,
        ReceiverDescriptor {
            label: Some("directional shadow"),
            dimension: ReceiverDimension::D2,
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
) -> [wgpu::RenderPipeline; 4] {
    let source = format!(
        "{ENVIRONMENT_WGSL}\n{}",
        include_str!("../shader/shadow_depth.wgsl")
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow caster"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
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
    let buffers = [
        Some(wgpu::VertexBufferLayout {
            array_stride: VERTEX_STRIDE,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertices,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: INSTANCE_STRIDE,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instances,
        }),
    ];
    PipelineKey::ALL.map(|key| {
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
                compare: wgpu::CompareFunction::Less,
                bias: wgpu::DepthBiasState::default(),
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
    })
}

#[derive(Debug)]
pub(super) struct ShadowMap {
    pub depth: DepthMap,
    pub uniform: wgpu::Buffer,
    pub group: wgpu::BindGroup,
}

impl ShadowMap {
    pub fn new(
        context: &crate::device::GpuContext,
        settings: DirectionalShadowSettings,
        layout: &wgpu::BindGroupLayout,
    ) -> Result<Self, DepthMapError> {
        let device = context.device();
        let max_resolution = u64::from(super::MAX_SHADOW_RESOLUTION);
        let depth = DepthMap::new(
            device,
            DepthMapDescriptor {
                label: Some("directional shadow depth"),
                resolution: settings.resolution,
                layers: 1,
                receiver_dimension: ReceiverDimension::D2,
                readback: false,
                budget: DepthMapBudget {
                    max_resolution: super::MAX_SHADOW_RESOLUTION,
                    max_layers: 1,
                    max_texture_bytes: max_resolution * max_resolution * 4,
                    max_readback_bytes: 0,
                },
            },
        )?;
        let sampler =
            worldrunner_render_shadow::comparison_sampler(device, wgpu::CompareFunction::LessEqual);
        let uniform = upload_buffer(
            device,
            context.queue(),
            "shadow uniform",
            wgpu::BufferUsages::UNIFORM,
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
        Ok(Self {
            depth,
            uniform,
            group,
        })
    }
}

pub(super) fn uniform_bytes(
    matrix: glam::Mat4,
    settings: DirectionalShadowSettings,
) -> [u8; SHADOW_UNIFORM_BYTES] {
    let mut bytes = [0; SHADOW_UNIFORM_BYTES];
    for (slot, value) in matrix
        .to_cols_array()
        .into_iter()
        .chain([settings.depth_bias, settings.normal_bias_meters, 0.0, 0.0])
        .enumerate()
    {
        bytes[slot * 4..slot * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
