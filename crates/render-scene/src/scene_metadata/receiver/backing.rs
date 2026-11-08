use super::ReceiverLayout;

/// One reservation-sized backing retained for the metadata allocation generation.
/// Active producer layouts select layers without replacing submitted storage.
#[derive(Clone, Debug)]
pub(crate) struct ReceiverBacking {
    pub(super) textures: [wgpu::Texture; 2],
}
impl ReceiverBacking {
    pub(super) fn new(device: &wgpu::Device, size: [u32; 2], reserved: ReceiverLayout) -> Self {
        let textures = [
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureFormat::Rg32Float,
        ]
        .map(|format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("receiver displacements"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: reserved.sides(),
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        Self { textures }
    }
}
