use worldrunner_render_post::{AttachmentView, FrameConfig, FrameError, ResourceRef};

pub(super) const NAMES: [&str; 3] = [
    "water receiver normals",
    "water receiver material",
    "water environment specular",
];

pub(super) fn textures(device: &wgpu::Device, config: FrameConfig) -> [wgpu::Texture; 3] {
    NAMES.map(|name| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name),
            size: wgpu::Extent3d {
                width: config.size[0],
                height: config.size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: config.samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | if config.samples == 1 {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        })
    })
}

pub(super) fn views(textures: &[wgpu::Texture; 3]) -> Result<Vec<AttachmentView>, FrameError> {
    NAMES
        .into_iter()
        .zip(textures)
        .map(|(name, texture)| AttachmentView::new(ResourceRef::new(name), texture))
        .collect()
}

pub(crate) fn empty_environment(device: &wgpu::Device) -> [wgpu::TextureView; 2] {
    [6, 1].map(|layers| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("unlit water environment"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(if layers == 6 {
                wgpu::TextureViewDimension::Cube
            } else {
                wgpu::TextureViewDimension::D2
            }),
            ..Default::default()
        })
    })
}

pub(super) fn environment_entries() -> [wgpu::BindGroupLayoutEntry; 4] {
    let mut frame = super::texture_entry(3);
    frame.ty = wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: wgpu::BufferSize::new(160),
    };
    let mut cube = super::texture_entry(4);
    cube.ty = wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: wgpu::TextureViewDimension::Cube,
        multisampled: false,
    };
    let mut lut = super::texture_entry(5);
    lut.ty = wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    };
    let mut sampler = super::texture_entry(6);
    sampler.ty = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
    [frame, cube, lut, sampler]
}
