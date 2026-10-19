use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};
use crate::pipeline::PipelineKey;
use crate::renderer::{DEPTH_FORMAT, ScenePipelines, build};
use crate::shader::ENVIRONMENT_WGSL;
use crate::upload::upload_buffer;

use super::DirectionalShadowSettings;

const SHADOW_UNIFORM_BYTES: usize = 80;

pub(super) const SHADOWED_WGSL: &str = concat!(
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/environment.wgsl")
);

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("directional shadow"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
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
        build(
            device,
            key,
            &layout,
            &buffers,
            wgpu::FragmentState {
                module: &module,
                entry_point: Some(if key.masked {
                    "fs_depth_masked"
                } else {
                    "fs_depth_opaque"
                }),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            },
        )
    })
}

#[derive(Debug)]
pub(super) struct ShadowMap {
    pub view: wgpu::TextureView,
    pub uniform: wgpu::Buffer,
    pub group: wgpu::BindGroup,
}

impl ShadowMap {
    pub fn new(
        context: &crate::device::GpuContext,
        settings: DirectionalShadowSettings,
        layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let device = context.device();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("directional shadow depth"),
            size: wgpu::Extent3d {
                width: settings.resolution,
                height: settings.resolution,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("directional shadow comparison"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
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
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        // The view and bind group retain the texture/sampler through wgpu handles.
        Self {
            view,
            uniform,
            group,
        }
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
