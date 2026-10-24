use super::{CheckedBindings, ContractError};

/// Layout limits are checked before pipeline-layout creation, per packet.
pub(super) fn validate(
    device: &wgpu::Device,
    groups: &[&CheckedBindings],
) -> Result<(), ContractError> {
    let limits = device.limits();
    if groups.len() > limits.max_bind_groups as usize {
        return Err(ContractError::Pipeline);
    }
    let entries: Vec<_> = groups
        .iter()
        .flat_map(|group| &group.layout_entries)
        .collect();
    let dynamic = |uniform| {
        entries
            .iter()
            .filter(|entry| {
                matches!(entry.ty,
        wgpu::BindingType::Buffer { ty, has_dynamic_offset: true, .. }
        if (ty == wgpu::BufferBindingType::Uniform) == uniform)
            })
            .count()
    };
    if dynamic(true) > limits.max_dynamic_uniform_buffers_per_pipeline_layout as usize
        || dynamic(false) > limits.max_dynamic_storage_buffers_per_pipeline_layout as usize
    {
        return Err(ContractError::Pipeline);
    }
    validate_stages(&limits, &entries)
}
fn validate_stages(
    limits: &wgpu::Limits,
    entries: &[&wgpu::BindGroupLayoutEntry],
) -> Result<(), ContractError> {
    for stage in [
        wgpu::ShaderStages::VERTEX,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::ShaderStages::COMPUTE,
    ] {
        let mut counts = [0_u32; 5];
        for entry in entries
            .iter()
            .filter(|entry| entry.visibility.contains(stage))
        {
            let kind = match entry.ty {
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    ..
                } => 0,
                wgpu::BindingType::Buffer { .. } => 1,
                wgpu::BindingType::Texture { .. } => 2,
                wgpu::BindingType::StorageTexture { .. } => 3,
                wgpu::BindingType::Sampler(_) => 4,
                _ => return Err(ContractError::Pipeline),
            };
            counts[kind] += 1;
        }
        let maxima = [
            limits.max_uniform_buffers_per_shader_stage,
            limits.max_storage_buffers_per_shader_stage,
            limits.max_sampled_textures_per_shader_stage,
            limits.max_storage_textures_per_shader_stage,
            limits.max_samplers_per_shader_stage,
        ];
        if counts
            .into_iter()
            .zip(maxima)
            .any(|(count, maximum)| count > maximum)
        {
            return Err(ContractError::Pipeline);
        }
    }
    Ok(())
}
