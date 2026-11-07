#[cfg(test)]
use super::{
    ColorTargetState, ColorWrites, Device, FragmentState, ObjectState, PipelineCompilationOptions,
    PipelineKey, PipelineLayout, RenderPipeline, TextureFormat, build_variant,
};
use super::{VertexBufferLayout, VertexStepMode};

/// The fixed unlit-alpha pipeline the dynamic quads drew with before they drew through the
/// program runtime. Only the byte-equality captures build it now, as their reference.
#[cfg(test)]
pub(super) fn build(
    device: &Device,
    target: (TextureFormat, u32),
    layout: &PipelineLayout,
    buffers: &[Option<VertexBufferLayout<'_>>; 2],
    module: &wgpu::ShaderModule,
    targets: &[Option<ColorTargetState>],
) -> RenderPipeline {
    let mut alpha_targets = targets.to_vec();
    for target in alpha_targets.iter_mut().skip(1).flatten() {
        target.write_mask = ColorWrites::empty();
    }
    alpha_targets[0] = Some(ColorTargetState {
        format: target.0,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    });
    let tint_attributes = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 9,
    }];
    let alpha_buffers = alpha_buffers(buffers, &tint_attributes);
    build_variant(
        device,
        layout,
        &alpha_buffers,
        FragmentState {
            module,
            entry_point: Some("fs_unlit"),
            compilation_options: PipelineCompilationOptions {
                constants: &[("SCENE_IMPOSTOR", 0.0)],
                ..Default::default()
            },
            targets: &alpha_targets,
        },
        ObjectState {
            samples: target.1,
            front_face: wgpu::FrontFace::Ccw,
            key: PipelineKey {
                coverage: crate::pipeline::MaskCoverage::Cutoff,
                sampling: crate::pipeline::MaterialSampling::Surface,
                relief: false,
                masked: false,
                double_sided: true,
            },
            depth_write: false,
            vertex_entry: "vs_unlit",
        },
    )
}

/// The vertex buffers of the unlit shader: the vertex and instance streams and a per-instance
/// tint. The retained-UI overlay's pipeline reads the same three.
pub(super) fn alpha_buffers<'a>(
    buffers: &[Option<VertexBufferLayout<'a>>; 2],
    tint_attributes: &'a [wgpu::VertexAttribute],
) -> [Option<VertexBufferLayout<'a>>; 3] {
    [
        buffers[0].clone(),
        buffers[1].clone(),
        Some(VertexBufferLayout {
            array_stride: 16,
            step_mode: VertexStepMode::Instance,
            attributes: tint_attributes,
        }),
    ]
}
