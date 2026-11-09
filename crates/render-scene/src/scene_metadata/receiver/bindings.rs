pub(crate) fn shadow_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = worldrunner_render_shadow::receiver_layout_entries(
        worldrunner_render_shadow::ReceiverDescriptor {
            label: Some("directional shadow"),
            dimension: worldrunner_render_shadow::ReceiverDimension::D2Array,
            texture_binding: 1,
            sampler_binding: 2,
            uniform_binding: 0,
            min_uniform_binding_size: None,
        },
    )
    .to_vec();
    entries.extend((3..5).map(|binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }));
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow with receiver displacements"),
        entries: &entries,
    })
}

pub(crate) fn group(
    device: &wgpu::Device,
    payload: &super::ReceiverPayload,
    uniform: &wgpu::Buffer,
    depth: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let views = payload.views();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow with receiver displacements"),
        layout: &shadow_layout(device),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&views[0]),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&views[1]),
            },
        ],
    })
}
