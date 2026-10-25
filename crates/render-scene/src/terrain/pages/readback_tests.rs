use super::composition::MaterialImage;

pub(super) struct MipRead {
    buffer: wgpu::Buffer,
    row: u32,
    height: u32,
}

pub(super) fn encode_reads(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    levels: &[MaterialImage],
    slot: u32,
) -> Vec<MipRead> {
    levels
        .iter()
        .enumerate()
        .map(|(mip, image)| {
            let [width, height] = image.extent();
            let row = (width * 4).div_ceil(256) * 256;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("page mip exact readback"),
                size: u64::from(row * height * 2),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: u32::try_from(mip).unwrap(),
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: slot * 2,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 2,
                },
            );
            MipRead {
                buffer,
                row,
                height,
            }
        })
        .collect()
}

pub(super) fn assert_reads(device: &wgpu::Device, reads: &[MipRead], expected: &[MaterialImage]) {
    assert_eq!(reads.len(), expected.len());
    for (mip, (read, image)) in reads.iter().zip(expected).enumerate() {
        let bytes = super::gpu_tests::read(device, &read.buffer);
        for (i, texel) in image.texels().iter().enumerate() {
            let index = u32::try_from(i).unwrap();
            let x = index % image.extent()[0];
            let y = index / image.extent()[0];
            let at = usize::try_from(y * read.row + x * 4).unwrap();
            let normal = at + usize::try_from(read.row * read.height).unwrap();
            assert_eq!(&bytes[at..at + 4], texel.color, "mip {mip} texel {i}");
            assert_eq!(
                &bytes[normal..normal + 4],
                texel.normal_roughness,
                "mip {mip} texel {i}"
            );
        }
    }
}
