use crate::device::GpuContext;
pub(super) fn math_layout(
    device: &wgpu::Device,
    uniform: Option<u32>,
    ground: bool,
) -> wgpu::PipelineLayout {
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    // Specialization can remove every frame read, but the harness still uploads its input.
    let mut entries = vec![entry(
        1,
        wgpu::BufferBindingType::Storage { read_only: false },
    )];
    if let Some(binding) = uniform {
        entries.push(entry(binding, wgpu::BufferBindingType::Uniform));
    }
    let mut groups = vec![
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        }),
    ];
    if ground {
        groups.push(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &ground_entries(entry(
                    7,
                    wgpu::BufferBindingType::Storage { read_only: true },
                )),
            }),
        );
    }
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &groups.iter().map(Some).collect::<Vec<_>>(),
        immediate_size: 0,
    })
}

pub(super) fn ground_binding(
    context: &GpuContext,
    pipeline: &wgpu::ComputePipeline,
    values: &[f32],
) -> wgpu::BindGroup {
    let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (values.len() * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bytes: Vec<_> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    context.queue().write_buffer(&buffer, 0, &bytes);
    let texture = crate::upload::placeholder_texture(context.device(), context.queue());
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = crate::upload::material_sampler(context.device());
    context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        })
}

fn ground_entries(buffer: wgpu::BindGroupLayoutEntry) -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries = vec![
        buffer,
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ];
    for binding in [9, 10] {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    entries
}
