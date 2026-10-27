use super::tests::{execute, fixture};
use super::*;
use crate::terrain::pages::{gpu_tests::read, residency_tests::context};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::{TerrainColorizationChannels, TerrainSplatFilter};

#[test]
fn odd_pages_and_gutters_match_direct_gpu_samples_with_height_mips() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let (terrain, resources) = fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let mut parts = crate::terrain::pages::residency_tests::settings().parts();
    parts.upload_bytes_per_frame = 100_000;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache = ResidentPages::new_layered(
        device,
        &authority,
        settings,
        2,
        AllocationBudget(1_000_000),
        source.input_bytes(),
    )
    .unwrap();
    let mut bases = Vec::new();
    for coordinate in [[0, 0], [1, 0]] {
        let key = PageKey::new(1, coordinate).unwrap();
        let positions = source.page_positions(settings, key);
        let direct = execute(&context, &source, &positions.0);
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, key).unwrap();
        let slot = pending.slot();
        source
            .encode_page(device, &mut frame, &cache, pending)
            .unwrap();
        let reads: Vec<_> = settings
            .mip_extents()
            .iter()
            .enumerate()
            .map(|(mip, extent)| {
                copy(
                    device,
                    &mut frame,
                    &cache,
                    [u32::try_from(mip).unwrap(), slot],
                    *extent,
                )
            })
            .collect();
        frame.finish().submit().unwrap();
        let levels: Vec<_> = reads
            .iter()
            .zip(settings.mip_extents())
            .map(|(buffer, extent)| unpack(device, buffer, extent))
            .collect();
        for (actual, expected) in levels[0].iter().zip(direct.chunks_exact(16)) {
            assert_eq!(&actual[..9], &expected[..9]);
        }
        assert_height_mips(&levels, &settings.mip_extents());
        bases.push(levels[0].clone());
    }
    assert_gutters(&bases);
}
fn assert_gutters(bases: &[Vec<[u8; 9]>]) {
    for y in 0..7 {
        assert_eq!(bases[0][y * 5 + 4], bases[1][y * 5 + 1]);
        assert_eq!(bases[0][y * 5 + 3], bases[1][y * 5]);
    }
}
pub(super) fn copy(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    cache: &ResidentPages,
    at: [u32; 2],
    [width, height]: [u32; 2],
) -> wgpu::Buffer {
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height * 3),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &cache.storage().texture,
            mip_level: at[0],
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: at[1] * 3,
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
            depth_or_array_layers: 3,
        },
    );
    buffer
}
pub(super) fn unpack(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    [width, height]: [u32; 2],
) -> Vec<[u8; 9]> {
    let bytes = read(device, buffer);
    let row = (width * 4).div_ceil(256) * 256;
    (0..width * height)
        .map(|i| {
            let offset = usize::try_from(i / width * row + i % width * 4).unwrap();
            let stride = usize::try_from(row * height).unwrap();
            std::array::from_fn(|channel| bytes[offset + (channel / 4) * stride + channel % 4])
        })
        .collect()
}
fn assert_height_mips(levels: &[Vec<[u8; 9]>], extents: &[[u32; 2]]) {
    for (images, sizes) in levels.windows(2).zip(extents.windows(2)) {
        let [ow, oh] = sizes[0];
        let [w, h] = sizes[1];
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0;
                for sy in y * oh / h..((y + 1) * oh).div_ceil(h) {
                    for sx in x * ow / w..((x + 1) * ow).div_ceil(w) {
                        let wx = ((sx + 1) * w).min((x + 1) * ow) - (sx * w).max(x * ow);
                        let wy = ((sy + 1) * h).min((y + 1) * oh) - (sy * h).max(y * oh);
                        sum += u32::from(images[0][usize::try_from(sy * ow + sx).unwrap()][8])
                            * wx
                            * wy;
                    }
                }
                assert_eq!(
                    u32::from(images[1][usize::try_from(y * w + x).unwrap()][8]),
                    (sum + ow * oh / 2) / (ow * oh)
                );
            }
        }
    }
}
