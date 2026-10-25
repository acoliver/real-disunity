use super::{GpuContext, post, resources, wgpu};
use resources::texture::{MipLevel, TextureKind, TextureLayer, TextureUpload};

pub fn upload(
    store: &mut resources::CanonicalTextureStore,
    id: &str,
    pixel: [u8; 4],
) -> wgpu::Texture {
    let mips = [MipLevel {
        width: 1,
        height: 1,
        bytes_per_row: 4,
        rows: 1,
        bytes: &pixel,
    }];
    let layers = [TextureLayer {
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 1,
        height: 1,
        mips: &mips,
    }];
    store
        .get_or_upload(
            id,
            "consumer immutable texture",
            wgpu::TextureUsages::COPY_SRC,
            || TextureUpload::new(&layers, 0, TextureKind::D2),
        )
        .unwrap()
}

pub fn read(context: &GpuContext, texture: &wgpu::Texture) -> [u8; 4] {
    let config = post::FrameConfig::new([1, 1], texture.format(), 1);
    let mut readback = post::Readback::new(context.device(), config, 256).unwrap();
    readback
        .read(context.device(), context.submission(), texture)
        .unwrap()
        .try_into()
        .unwrap()
}
