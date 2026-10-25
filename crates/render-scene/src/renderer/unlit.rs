use super::{
    ColorTargetState, ColorWrites, Device, FragmentState, ObjectState, PipelineCompilationOptions,
    PipelineKey, PipelineLayout, RenderPipeline, TextureFormat, VertexBufferLayout, VertexStepMode,
    build_variant,
};

pub(super) fn build(
    device: &Device,
    target: (TextureFormat, u32),
    layout: &PipelineLayout,
    buffers: &[Option<VertexBufferLayout<'_>>; 2],
    module: &wgpu::ShaderModule,
) -> RenderPipeline {
    let alpha_targets = [Some(ColorTargetState {
        format: target.0,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];
    let tint_attributes = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 9,
    }];
    let alpha_buffers = [
        buffers[0].clone(),
        buffers[1].clone(),
        Some(VertexBufferLayout {
            array_stride: 16,
            step_mode: VertexStepMode::Instance,
            attributes: &tint_attributes,
        }),
    ];
    build_variant(
        device,
        layout,
        &alpha_buffers,
        FragmentState {
            module,
            entry_point: Some("fs_unlit"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &alpha_targets,
        },
        ObjectState {
            samples: target.1,
            key: PipelineKey {
                masked: false,
                double_sided: true,
            },
            depth_write: false,
            vertex_entry: "vs_unlit",
        },
    )
}
