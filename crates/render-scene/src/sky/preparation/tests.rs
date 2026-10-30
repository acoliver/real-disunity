#[path = "base_arithmetic_tests.rs"]
mod base_arithmetic_tests;

#[test]
fn specular_intensity_matches_closed_form_single_sample_without_changing_cube() {
    assert_eq!(integration_source(1_000_000), SOURCE);
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let source = source_cube(&context);
    for intensity in [0, 500_000, 1_000_000, 2_000_000, 64_000_000] {
        let settings = EnvironmentPreparation {
            samples: 1,
            specular_intensity_micro: intensity,
            ..settings()
        };
        let mut maps = None;
        let prepared = context
            .submission()
            .prepare(device, |encoder, _| {
                maps = Some(encode(
                    device,
                    encoder,
                    &source,
                    settings,
                    AllocationBudget(1_000_000),
                )?);
                Ok::<_, PreparationError>(())
            })
            .unwrap();
        prepared.submit().unwrap();
        let maps = maps.unwrap();
        assert_eq!(maps.specular_intensity_micro(), 1_000_000);
        let data = read_texture(device, context.submission(), &maps.lut, 0);
        for x in 0_u16..4 {
            let no_v = (f64::from(x) + 0.5) / 4.0;
            let roughness = 0.125_f64;
            // The single half-vector is exactly +Z, so no trigonometric approximation enters.
            let weight =
                no_v / (no_v * no_v * (1.0 - roughness.powi(4)) + roughness.powi(4)).sqrt();
            let fresnel = (1.0 - no_v).powi(5);
            let expected =
                [1.0 - fresnel, fresnel].map(|v| v * weight * f64::from(intensity) / 1e6);
            for (channel, expected) in expected.into_iter().enumerate() {
                let offset = usize::from(x) * 8 + channel * 2;
                let actual = half(u16::from_le_bytes(
                    data[offset..offset + 2].try_into().unwrap(),
                ));
                assert!(
                    (f64::from(actual) - expected).abs()
                        <= expected.abs() * 2.0_f64.powi(-10) + 2e-7
                );
            }
        }
        verify_constant(device, context.submission(), &maps.cube);
    }
    assert!(
        EnvironmentPreparation {
            specular_intensity_micro: 64_000_001,
            ..settings()
        }
        .allocations()
        .is_err()
    );
}

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
        convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        cube_size: 4,
        source_lod_tail: 1,
        mip_count: 3,
        specular_intensity_micro: 1_000_000,
        lut_size: 4,
        samples: 32,
    }
}
#[test]
fn bounded_preparation_settings() {
    assert!(settings().allocations().is_ok());
    for config in [
        EnvironmentPreparation {
            convention: worldrunner_render_plan::SceneSpecularConvention::default(),
            samples: 0,
            ..settings()
        },
        EnvironmentPreparation {
            convention: worldrunner_render_plan::SceneSpecularConvention::default(),
            mip_count: 4,
            ..settings()
        },
        EnvironmentPreparation {
            convention: worldrunner_render_plan::SceneSpecularConvention::default(),
            cube_size: 3,
            source_lod_tail: 1,
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
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    verify_constant(device, context.submission(), &maps.cube);
    verify_lut(device, context.submission(), &maps.lut, settings());
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
    settings: EnvironmentPreparation,
) {
    let data = read_texture(device, authority, lut, 0);
    for y in 0..4u16 {
        for x in 0..4u16 {
            let input = (f32::from(y) + 0.5) / 4.0;
            let roughness = if settings.convention.roughness
                == worldrunner_render_plan::SceneRoughnessRemap::LinearAlpha
            {
                input.sqrt()
            } else {
                input
            };
            let expected = reference::integral(
                (f32::from(x) + 0.5) / 4.0,
                roughness,
                settings.samples,
                settings.convention.visibility
                    == worldrunner_render_plan::SceneSpecularVisibility::SchlickSmith,
            );
            let offset = usize::from(y) * 256 + usize::from(x) * 8;
            for (channel, expected) in expected.into_iter().enumerate() {
                let index = offset + channel * 2;
                let value = half(u16::from_le_bytes([data[index], data[index + 1]]));
                assert!(
                    (f64::from(value) - expected).abs() < 0.001,
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
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    recv.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
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

#[test]
fn selected_environment_conventions_match_reference_integrals() {
    use worldrunner_render_plan::{
        SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularVisibility,
    };
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let source = source_cube(&context);
    for roughness in [
        SceneRoughnessRemap::Perceptual045,
        SceneRoughnessRemap::Perceptual04,
        SceneRoughnessRemap::LinearAlpha,
    ] {
        for visibility in [
            SceneSpecularVisibility::SmithCorrelated,
            SceneSpecularVisibility::SchlickSmith,
        ] {
            let settings = EnvironmentPreparation {
                convention: SceneSpecularConvention {
                    roughness,
                    visibility,
                    ..Default::default()
                },
                ..settings()
            };
            let mut maps = None;
            context
                .submission()
                .prepare(context.device(), |encoder, _| {
                    maps = Some(encode(
                        context.device(),
                        encoder,
                        &source,
                        settings,
                        AllocationBudget(1_000_000),
                    )?);
                    Ok::<_, PreparationError>(())
                })
                .unwrap()
                .submit()
                .unwrap();
            let maps = maps.unwrap();
            context
                .device()
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
            verify_constant(context.device(), context.submission(), &maps.cube);
            verify_lut(context.device(), context.submission(), &maps.lut, settings);
            assert_eq!(maps.convention(), settings.convention);
        }
    }
}

#[path = "directional_tests.rs"]
mod directional_tests;
#[path = "../../../../render-lighting/src/tests/reference.rs"]
mod reference;

#[path = "policy_tests.rs"]
mod policy_tests;
