//! Schedules "temporal create locks", the compute pass at the internal extent `R` that marks
//! thin-feature candidates, see `taa_thin.wgsl`. The temporal upscale pass gathers the field to the
//! output extent.
use crate::{Operation, Stage, StageId};

/// Stage name.
pub const STAGE: &str = "temporal create locks";

/// Format of the candidate field: strength and compressed luminance. Two 16 bit float channels are
/// not a core storage format, so the field is written at 32 bits.
pub const CANDIDATE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

/// The pass reading `color` (any float format, at `R`) and the candidate texture it writes.
pub fn stage(
    device: &wgpu::Device,
    color: &wgpu::TextureView,
    size: [u32; 2],
) -> (Stage, wgpu::Texture) {
    let out = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("temporal thin candidates"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: CANDIDATE_FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(STAGE),
        source: wgpu::ShaderSource::Wgsl(include_str!("taa_thin.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(STAGE),
        layout: None,
        module: &module,
        entry_point: Some("create_locks"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let view = out.create_view(&wgpu::TextureViewDescriptor::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGE),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(color),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    });
    let stage = Stage {
        id: StageId::new(STAGE, 0),
        label: STAGE,
        enabled: true,
        operation: Operation::Compute {
            pipeline,
            groups: vec![group],
            dispatch: [size[0].div_ceil(8), size[1].div_ceil(8), 1],
        },
    };
    (stage, out)
}
