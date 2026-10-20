/// A receiver's shader texture type; a single layer need not imply a D2 binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverDimension {
    D2,
    D2Array,
}

impl From<ReceiverDimension> for wgpu::TextureViewDimension {
    fn from(value: ReceiverDimension) -> Self {
        match value {
            ReceiverDimension::D2 => Self::D2,
            ReceiverDimension::D2Array => Self::D2Array,
        }
    }
}

/// The caller's existing shader ABI, without imposing binding order or uniform encoding.
#[derive(Clone, Copy, Debug)]
pub struct ReceiverDescriptor<'a> {
    pub label: Option<&'a str>,
    pub dimension: ReceiverDimension,
    pub texture_binding: u32,
    pub sampler_binding: u32,
    pub uniform_binding: u32,
    pub min_uniform_binding_size: Option<std::num::NonZeroU64>,
}

/// Creates a fragment-visible depth texture/comparison sampler/uniform layout.
/// Binding indices must be distinct; wgpu validates the caller's shader ABI.
pub fn receiver_layout(
    device: &wgpu::Device,
    descriptor: ReceiverDescriptor<'_>,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: descriptor.label,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: descriptor.texture_binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: descriptor.dimension.into(),
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: descriptor.sampler_binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: descriptor.uniform_binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: descriptor.min_uniform_binding_size,
                },
                count: None,
            },
        ],
    })
}

/// Linear clamp comparison sampler. The caller explicitly preserves its comparison rule.
pub fn comparison_sampler(device: &wgpu::Device, compare: wgpu::CompareFunction) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow comparison sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        compare: Some(compare),
        ..Default::default()
    })
}
