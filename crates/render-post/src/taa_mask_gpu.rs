//! Schedules "temporal mask dilate": a fullscreen pass at the internal extent `R` that writes the
//! per-slot prepared mask, the 3x3 maximum per channel of the producer mask, see `taa_mask.wgsl`.
//! It exists only when the frame targets carry a producer mask.
use crate::{Stage, StageId, StageTarget, TEMPORAL_MASK_FORMAT};

/// Stage name.
pub const STAGE: &str = "temporal mask dilate";

/// The pass reading `mask` (a [`TEMPORAL_MASK_FORMAT`] view at `size`) and the prepared mask
/// texture it writes. The caller keeps the texture alive with the stage set.
pub fn stage(
    device: &wgpu::Device,
    mask: &wgpu::TextureView,
    size: [u32; 2],
) -> (Stage, wgpu::Texture) {
    let out = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("temporal prepared mask"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEMPORAL_MASK_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(STAGE),
        source: wgpu::ShaderSource::Wgsl(include_str!("taa_mask.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(STAGE),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("dilate"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(TEMPORAL_MASK_FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGE),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(mask),
        }],
    });
    let target = StageTarget::Texture(out.create_view(&wgpu::TextureViewDescriptor::default()));
    let stage = Stage::fullscreen(StageId::new(STAGE, 0), &pipeline, &[&group], target);
    (stage, out)
}
