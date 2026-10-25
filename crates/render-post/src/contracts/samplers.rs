use super::ContractError;

/// Retains the classification derived from the actual sampler descriptor.
#[derive(Clone, Debug)]
pub struct CheckedSampler {
    pub(super) sampler: wgpu::Sampler,
    kind: wgpu::SamplerBindingType,
}
impl CheckedSampler {
    pub fn new(device: &wgpu::Device, descriptor: &wgpu::SamplerDescriptor<'_>) -> Self {
        let kind = if descriptor.compare.is_some() {
            wgpu::SamplerBindingType::Comparison
        } else if descriptor.mag_filter == wgpu::FilterMode::Linear
            || descriptor.min_filter == wgpu::FilterMode::Linear
            || descriptor.mipmap_filter == wgpu::MipmapFilterMode::Linear
            || descriptor.anisotropy_clamp > 1
        {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        };
        Self {
            sampler: device.create_sampler(descriptor),
            kind,
        }
    }
    pub const fn kind(&self) -> wgpu::SamplerBindingType {
        self.kind
    }
}
#[derive(Clone, Debug)]
pub struct SamplerBinding {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub sampler: CheckedSampler,
}
pub(super) fn append_layouts(
    device: &wgpu::Device,
    entries: &mut Vec<wgpu::BindGroupLayoutEntry>,
    samplers: &[SamplerBinding],
) -> Result<(), ContractError> {
    for entry in samplers {
        if entry.binding >= device.limits().max_bindings_per_bind_group
            || entry.visibility.is_empty()
            || wgpu::ShaderStages::from_bits(entry.visibility.bits()).is_none()
            || entries.iter().any(|old| old.binding == entry.binding)
        {
            return Err(ContractError::Pipeline);
        }
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: entry.binding,
            visibility: entry.visibility,
            ty: wgpu::BindingType::Sampler(entry.sampler.kind),
            count: None,
        });
    }
    for stage in [
        wgpu::ShaderStages::VERTEX,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::ShaderStages::COMPUTE,
    ] {
        if samplers
            .iter()
            .filter(|entry| entry.visibility.contains(stage))
            .count()
            > device.limits().max_samplers_per_shader_stage as usize
        {
            return Err(ContractError::Pipeline);
        }
    }
    Ok(())
}
