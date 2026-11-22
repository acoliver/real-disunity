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
