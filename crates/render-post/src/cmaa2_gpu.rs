//! Schedules the CMAA2 passes. The shader is `cmaa2.wgsl`.
use crate::{Cmaa2Settings, Stage, StageId, StageSet, StageTarget};

pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    _size: [u32; 2],
    cmaa2: Cmaa2Settings,
    output: (wgpu::TextureFormat, StageTarget),
) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("CMAA2"),
        source: wgpu::ShaderSource::Wgsl(include_str!("cmaa2.wgsl").into()),
    });
    let constants = cmaa2.constants();
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cmaa2_present"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("cmaa2_present"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
            targets: &[Some(output.0.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cmaa2_present"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::TextureView(source),
        }],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new("cmaa2_present", 0),
        &pipeline,
        &[&group],
        output.1,
    ));
}
