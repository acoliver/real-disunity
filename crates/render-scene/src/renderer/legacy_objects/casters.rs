//! The fixed object shadow casters, as `shadows::resources::depth_pipelines` built them before
//! the casters drew through the program runtime: one depth pipeline for each of the eight key
//! families that differ in a directional map (masked or not, impostor or not, two cull modes),
//! shared by the keys of each family, over the frozen environment and caster sources.

use wgpu::RenderPipeline;
use worldrunner_render_shadow::{
    DepthFragmentState, DepthPipelineDescriptor, DirectionalShadowConfig,
};

use crate::geometry::{instance_attributes, vertex_attributes};
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::renderer::depth::DepthMode;

/// The caster source as `shadows::depth_source` composed it, over the snapshots.
pub(crate) fn caster_source() -> String {
    DepthMode::Independent
        .vertex_source(&format!(
            "{}\n{}",
            super::source::ENVIRONMENT,
            include_str!("../../object/fixtures/shadow_depth.wgsl")
        ))
        .into_owned()
}

/// The caster pipeline of each key, built against the frame and material layouts of `pipelines`
/// with the depth test and bias of `configuration`.
pub(crate) fn caster_pipelines(
    device: &wgpu::Device,
    pipelines: &ScenePipelines,
    configuration: DirectionalShadowConfig,
) -> [RenderPipeline; PipelineKey::ALL.len()] {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow caster"),
        source: wgpu::ShaderSource::Wgsl(caster_source().into()),
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
    let buffers = crate::renderer::vertex_buffers(&vertices, &instances);
    // Relief is deliberately flat in directional maps, so its pipelines are identical.
    let objects: [_; 8] = std::array::from_fn(|index| {
        let key = PipelineKey::ALL[(index / 4) * 16 + index % 4];
        caster_pipeline(device, key, (&layout, &module), &buffers, configuration)
    });
    std::array::from_fn(|index| {
        let key = PipelineKey::ALL[index];
        objects[usize::from(key.sampling.is_impostor()) * 4 + index % 4].clone()
    })
}

fn caster_pipeline(
    device: &wgpu::Device,
    key: PipelineKey,
    (layout, module): (&wgpu::PipelineLayout, &wgpu::ShaderModule),
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    configuration: DirectionalShadowConfig,
) -> RenderPipeline {
    worldrunner_render_shadow::create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: Some("environment"),
            layout,
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: key.cull_mode(),
                ..Default::default()
            },
            compare: configuration.caster_compare,
            bias: configuration.caster_bias,
            fragment: Some(DepthFragmentState {
                module,
                entry_point: Some(if key.masked {
                    "fs_depth_masked"
                } else {
                    "fs_depth_opaque"
                }),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &key.fragment_constants(1),
                    ..Default::default()
                },
            }),
        },
    )
}
