//! The one-texel textures the engine lends as neutral inputs.
use super::EnginePlaceholder;

impl EnginePlaceholder {
    /// The texture of this placeholder, with its texel written. Rgba8 placeholders go through
    /// the stock upload; the zero placeholders are made here in their own formats.
    pub(super) fn create(self, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
        use wgpu::TextureDimension::{D2, D3};
        use wgpu::TextureFormat::{R8Unorm, R16Float, Rg8Unorm};
        match self {
            Self::White => crate::upload::placeholder_texture(device, queue),
            Self::Black => texel(device, queue, [0, 0, 0, 255]),
            Self::FlatNormal => texel(device, queue, [128, 128, 255, 255]),
            Self::ZeroR16Float => zero(device, queue, R16Float, D2),
            Self::ZeroR8 => zero(device, queue, R8Unorm, D2),
            Self::ZeroRg8 => zero(device, queue, Rg8Unorm, D2),
            Self::ZeroRg8Volume => zero(device, queue, Rg8Unorm, D3),
        }
    }
}

fn texel(device: &wgpu::Device, queue: &wgpu::Queue, texel: [u8; 4]) -> wgpu::Texture {
    crate::upload::texel_texture(device, queue, "engine placeholder", texel)
}

/// A 1x1x1 texture of `format` whose one texel is all zero bits, which is 0 in every channel
/// of the unorm and float formats the placeholders use.
fn zero(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureDimension,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("engine placeholder"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let block = format
        .block_copy_size(None)
        .expect("a color format has a texel size");
    queue.write_texture(
        texture.as_image_copy(),
        &vec![0; block as usize],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(block),
            rows_per_image: Some(1),
        },
        size,
    );
    texture
}
