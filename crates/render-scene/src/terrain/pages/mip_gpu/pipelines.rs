use super::PageWriter;

pub(super) fn create(device: &wgpu::Device, input_bytes: u64, products: u32) -> PageWriter {
    let storage = wgpu::BindingType::StorageTexture {
        access: wgpu::StorageTextureAccess::WriteOnly,
        format: wgpu::TextureFormat::Rgba8Uint,
        view_dimension: wgpu::TextureViewDimension::D2Array,
    };
    let buffer = wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: wgpu::BufferSize::new(input_bytes),
    };
    let texture = wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Uint,
        view_dimension: wgpu::TextureViewDimension::D2Array,
        multisampled: false,
    };
    PageWriter {
        input_bytes,
        products,
        store: pipeline(
            device,
            [buffer, storage],
            include_str!("../page_store.wgsl"),
            "store",
            products,
        ),
        reduce: pipeline(
            device,
            [texture, storage],
            include_str!("../mip_gpu.wgsl"),
            "reduce",
            products,
        ),
    }
}
fn pipeline(
    device: &wgpu::Device,
    types: [wgpu::BindingType; 2],
    shader: &str,
    entry: &'static str,
    products: u32,
) -> wgpu::ComputePipeline {
    let entries: Vec<_> = types
        .into_iter()
        .enumerate()
        .map(|(i, ty)| wgpu::BindGroupLayoutEntry {
            binding: u32::try_from(i).expect("two bindings"),
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        })
        .collect();
    let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("terrain page kernel"),
        entries: &entries,
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(&group)],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("terrain page kernel"),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    worldrunner_render_post::compute_pipeline(
        device,
        worldrunner_render_post::ComputeDescriptor {
            label: Some("terrain page kernel"),
            layout: &layout,
            module: &module,
            entry,
            constants: &[("layered_products", f64::from(u8::from(products == 3)))],
        },
    )
}
