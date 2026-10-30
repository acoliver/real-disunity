use super::LayeredGpuSource;
use crate::device::GpuContext;
use std::fmt::Write;

pub(super) fn report(
    context: &GpuContext,
    sources: &[LayeredGpuSource],
    positions: &[[f32; 2]],
    samples: [&[u8]; 3],
) -> String {
    let adapter = context.adapter();
    let mut text = format!(
        "\nadapter={:?}\nadapter features={:?}\ndevice features={:?}\ndownlevel={:?}\nANISOTROPIC_FILTERING={} (wgpu exposes this as a DownlevelFlags capability, not a Features bit)\npositions={positions:?}\nworld gradients: grazing dx=[0.00390625, 0], dy=[0, 0.0625]; baseline dx=[0.00390625, 0], dy=[0, 0.00390625]\nsource UV: (world + [0.13, -0.07]) / 1, rotation=0; 1x grazing shader gradients=[0.0625, 0], [0, 0.0625]; 16x uses actual UV gradients\n",
        adapter.get_info(),
        adapter.features(),
        context.device().features(),
        adapter.get_downlevel_capabilities(),
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING),
    );
    for (label, bytes) in ["1x grazing", "16x grazing", "1x isotropic baseline"]
        .into_iter()
        .zip(samples)
    {
        let values: Vec<[u8; 16]> = bytes
            .chunks_exact(16)
            .map(|v| v.try_into().unwrap())
            .collect();
        writeln!(
            text,
            "{label} raw packed records (RGBA, normal/roughness, height, padding)={values:?}"
        )
        .unwrap();
    }
    for source in sources {
        source_mips(&mut text, context, source, positions);
    }
    text
}

fn source_mips(
    text: &mut String,
    context: &GpuContext,
    source: &LayeredGpuSource,
    positions: &[[f32; 2]],
) {
    let resident = source.resident(context.device(), context.queue()).unwrap();
    writeln!(
        text,
        "requested anisotropy={} actual created sampler descriptors={:?}",
        source.anisotropy.level(),
        resident.sampler_descriptors
    )
    .unwrap();
    // Image zero is the checker albedo, with its own array and layer zero.
    let texture = &resident.textures[0];
    writeln!(
        text,
        "GPU source albedo: format={:?}, extent={:?}, mips={}",
        texture.format(),
        texture.size(),
        texture.mip_level_count()
    )
    .unwrap();
    for mip in [0, 1, 2, 3, 4, 8] {
        let bytes = read_mip(context, texture, mip);
        let near = nearby(&bytes, positions, mip);
        writeln!(
            text,
            "GPU mip={mip}, near each position: [x,y], RGBA at x and x+1={near:?}"
        )
        .unwrap();
    }
}

fn read_mip(context: &GpuContext, texture: &wgpu::Texture, mip: u32) -> Vec<u8> {
    let size = 256_u32 >> mip;
    let stride = (size * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let device = context.device();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("anisotropy diagnostic source mip readback"),
        size: u64::from(stride * size),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: mip,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size),
            },
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    context.queue().submit([encoder.finish()]);
    crate::terrain::pages::gpu_tests::read(device, &buffer)
}

fn nearby(bytes: &[u8], positions: &[[f32; 2]], mip: u32) -> Vec<([u32; 2], [[u8; 4]; 2])> {
    let size = 256_u16 >> mip;
    let stride =
        usize::from(size * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
    positions
        .iter()
        .map(|point| {
            let uv = glam::Vec2::from_array(*point) + glam::Vec2::new(0.13, -0.07);
            let coordinate = (uv.rem_euclid(glam::Vec2::ONE) * f32::from(size)).as_uvec2();
            let values = std::array::from_fn(|i| {
                let x = (coordinate.x as usize + i) % usize::from(size);
                let offset = coordinate.y as usize * stride + x * 4;
                bytes[offset..offset + 4].try_into().unwrap()
            });
            (coordinate.to_array(), values)
        })
        .collect()
}
