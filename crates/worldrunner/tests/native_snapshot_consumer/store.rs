use worldrunner::scene::{
    gpu::{device::GpuContext, wgpu},
    post,
    resources::{
        CanonicalTextureStore,
        texture::{MipLevel, TextureKind, TextureLayer, TextureUpload},
    },
};

pub fn retain(
    store: &mut CanonicalTextureStore,
    size: [u32; 2],
    id: &str,
    pixels: &[u8],
) -> wgpu::Texture {
    let mips = [MipLevel {
        width: size[0],
        height: size[1],
        bytes_per_row: size[0] * 4,
        rows: size[1],
        bytes: pixels,
    }];
    let layers = [TextureLayer {
        format: super::stage::FORMAT,
        width: size[0],
        height: size[1],
        mips: &mips,
    }];
    store
        .get_or_upload(
            id,
            "native capture immutable retained pixels",
            wgpu::TextureUsages::COPY_SRC,
            || TextureUpload::new(&layers, 0, TextureKind::D2),
        )
        .unwrap()
}

pub fn read(context: &GpuContext, texture: &wgpu::Texture, size: [u32; 2]) -> Vec<u8> {
    let mut readback = post::Readback::new(
        context.device(),
        post::FrameConfig::new(size, texture.format(), 1),
        1_048_576,
    )
    .unwrap();
    readback
        .read(context.device(), context.submission(), texture)
        .unwrap()
}
