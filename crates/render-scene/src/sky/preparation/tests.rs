use super::*;

#[test]
fn prefilter_uses_source_mip_footprints_but_mirror_keeps_mip_zero() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let source = mip_distinct_source(&context);
    let mut maps = None;
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            maps = Some(encode(
                device,
                encoder,
                &source,
                settings(),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let maps = maps.unwrap();
    let mirror = read_texture(device, context.submission(), &maps.cube, 0);
    assert_eq!(&mirror[..8], &[0, 0x40, 0, 0x40, 0, 0x40, 0, 0x3c]);
    let rough = read_texture(device, context.submission(), &maps.cube, 2);
    let red = half(u16::from_le_bytes([rough[0], rough[1]]));
    assert!(
        red > 7.9,
        "rough integration must use coarse source mips: {red}"
    );
}

fn settings() -> EnvironmentPreparation {
    EnvironmentPreparation {
        cube_size: 4,
        mip_count: 3,
        lut_size: 4,
        samples: 32,
    }
}
#[test]
fn bounded_preparation_settings() {
    assert!(settings().allocations().is_ok());
    for config in [
        EnvironmentPreparation {
            samples: 0,
            ..settings()
        },
        EnvironmentPreparation {
            mip_count: 4,
            ..settings()
        },
        EnvironmentPreparation {
            cube_size: 3,
            ..settings()
        },
    ] {
        assert!(config.allocations().is_err());
    }
    let module = naga::front::wgsl::parse_str(SOURCE).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn cancellation_does_not_publish_environment_resources() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let source = source_cube(&context);
    let mut maps = None;
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            maps = Some(encode(
                device,
                encoder,
                &source,
                settings(),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    let cancelled = maps.take().unwrap();
    assert!(cancelled.views().is_err());
    prepared.cancel();
    assert_eq!(cancelled.status(), TransactionStatus::Cancelled);
    assert!(cancelled.views().is_err());
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            maps = Some(encode(
                device,
                encoder,
                &source,
                settings(),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let maps = maps.unwrap();
    assert!(maps.views().is_ok());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    verify_constant(device, context.submission(), &maps.cube);
    verify_lut(device, context.submission(), &maps.lut);
}
fn verify_constant(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    cube: &wgpu::Texture,
) {
    let data = read_texture(device, authority, cube, 2);
    for face in 0..6 {
        assert_eq!(
            &data[face * 256..face * 256 + 8],
            &[0, 0x40, 0, 0x44, 0, 0x48, 0, 0x3c]
        );
    }
}
fn verify_lut(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    lut: &wgpu::Texture,
) {
    let data = read_texture(device, authority, lut, 0);
    for y in 0..4u16 {
        for x in 0..4u16 {
            let expected = worldrunner_render_lighting::ibl::brdf_integral(
                (f32::from(x) + 0.5) / 4.0,
                (f32::from(y) + 0.5) / 4.0,
                settings().samples,
            );
            let offset = usize::from(y) * 256 + usize::from(x) * 8;
            for (channel, expected) in expected.into_iter().enumerate() {
                let index = offset + channel * 2;
                let value = half(u16::from_le_bytes([data[index], data[index + 1]]));
                assert!(
                    (value - expected).abs() < 0.001,
                    "LUT ({x},{y}) channel {channel}: {value} != {expected}"
                );
            }
        }
    }
}
fn half(bits: u16) -> f32 {
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f32::from(bits & 1023) / 1024.0;
    assert_eq!(bits & 0x8000, 0);
    if exponent == 0 {
        fraction * 2.0f32.powi(-14)
    } else {
        (1.0 + fraction) * 2.0f32.powi(exponent - 15)
    }
}
fn read_texture(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    texture: &wgpu::Texture,
    mip: u32,
) -> Vec<u8> {
    let size = wgpu::Extent3d {
        width: texture.width() >> mip,
        height: texture.height() >> mip,
        depth_or_array_layers: texture.depth_or_array_layers(),
    };
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("environment readback"),
        size: u64::from(256 * size.height * size.depth_or_array_layers),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let prepared = authority
        .prepare(device, |encoder, _| {
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
                        bytes_per_row: Some(256),
                        rows_per_image: Some(size.height),
                    },
                },
                size,
            );
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let (send, recv) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    data
}

fn source_cube(context: &crate::device::GpuContext) -> wgpu::TextureView {
    let device = context.device();
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("constant source"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Powers of two remain exact in half-float and avoid transfer-function boundaries.
    let texel = [0x4000u16, 0x4400, 0x4800, 0x3c00]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let bytes = texel.repeat(4 * 4 * 6);
    context.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(4),
        },
        source.size(),
    );
    source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

fn mip_distinct_source(context: &crate::device::GpuContext) -> wgpu::TextureView {
    let device = context.device();
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mip-distinct environment"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 6,
        },
        mip_level_count: 7,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for mip in 0..7 {
        let size = 64 >> mip;
        let value = if mip == 0 { 0x4000u16 } else { 0x4800 };
        let texel = [value, value, value, 0x3c00]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &texel.repeat((size * size * 6) as usize),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 8),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
        );
    }
    source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

#[test]
fn encode_rejects_an_under_budget_environment_job() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let source = source_cube(&context);
    let required = settings()
        .allocations()
        .unwrap()
        .validate(&device.limits())
        .unwrap();
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            assert!(matches!(
                encode(
                    device,
                    encoder,
                    &source,
                    settings(),
                    AllocationBudget(required - 1)
                ),
                Err(PreparationError::Budget)
            ));
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    prepared.cancel();
}
