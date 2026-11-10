use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Device,
    ShaderStages, TextureSampleType, TextureViewDimension,
};

/// The previous-frame block of the frame group, in bytes. It is the size
/// `frame::temporal::PREVIOUS_FRAME_BYTES` had at the freeze.
const PREVIOUS_FRAME_BYTES: u64 = 80;

/// The frame layout, as `upload::frame_bind_group_layout` had it at the freeze.
pub(super) fn frame_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("frame"),
        entries: &[
            uniform(0, ShaderStages::VERTEX_FRAGMENT, None),
            uniform(1, ShaderStages::FRAGMENT, None),
            decal_texture(2),
            decal_texture(4),
            decal_texture(5),
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            uniform(
                6,
                ShaderStages::VERTEX_FRAGMENT,
                wgpu::BufferSize::new(PREVIOUS_FRAME_BYTES),
            ),
        ],
    })
}

/// The material layout, as `upload::material_bind_group_layout` had it at the freeze.
pub(super) fn material_layout(device: &Device) -> BindGroupLayout {
    let mut entries = vec![uniform(0, ShaderStages::VERTEX_FRAGMENT, None)];
    entries.extend((1..=5).map(|binding| texture(binding, ShaderStages::FRAGMENT)));
    entries.extend([
        texture(8, ShaderStages::FRAGMENT),
        texture(9, ShaderStages::VERTEX),
        texture(10, ShaderStages::VERTEX),
    ]);
    entries.push(BindGroupLayoutEntry {
        binding: 6,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    entries.push(BindGroupLayoutEntry {
        binding: 7,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    });
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("material"),
        entries: &entries,
    })
}

fn uniform(
    binding: u32,
    visibility: ShaderStages,
    min_binding_size: Option<wgpu::BufferSize>,
) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size,
        },
        count: None,
    }
}

fn texture(binding: u32, visibility: ShaderStages) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn decal_texture(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}
