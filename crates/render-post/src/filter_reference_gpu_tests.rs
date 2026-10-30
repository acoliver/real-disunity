use super::filter_reference::{Image, fxaa, half_bits, prefilter, tent, upsample};
use crate::{FrameConfig, Readback};

fn upload(device: &wgpu::Device, queue: &wgpu::Queue, image: &Image) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scalar filter fixture"),
        size: wgpu::Extent3d {
            width: image.size[0].into(),
            height: image.size[1].into(),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: Vec<_> = image
        .pixels
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(u32::from(image.size[0]) * 16),
            rows_per_image: None,
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn run(
    gpu: (&wgpu::Device, &wgpu::Queue),
    entry: &str,
    inputs: (&Image, Option<&Image>),
    expected: &Image,
    format: wgpu::TextureFormat,
    constants: &[(&str, f64)],
) {
    let actual = render(gpu, entry, inputs, expected, format, constants);
    let bytes = expected_bytes(expected, format, entry);
    if format == wgpu::TextureFormat::Rgba16Float {
        // Float16 stores may round toward zero where the scalar reference rounds
        // to nearest; see `half_words_within_one_step`. The references are
        // exactly representable, so results sit on the reference or one step low.
        assert_eq!(actual.len(), bytes.len(), "{entry}, {format:?}");
        for (index, (a, b)) in actual
            .chunks_exact(2)
            .zip(bytes.chunks_exact(2))
            .enumerate()
        {
            let (a, b) = (
                u16::from_le_bytes([a[0], a[1]]),
                u16::from_le_bytes([b[0], b[1]]),
            );
            assert!(
                a == b || (a < b && crate::tone_map_gpu_tests::half_words_within_one_step(a, b)),
                "{entry}, {format:?}, word {index}: {a:#06x} versus {b:#06x}"
            );
        }
        return;
    }
    assert_eq!(actual, bytes, "{entry}, {format:?}");
}

pub(super) fn render(
    gpu: (&wgpu::Device, &wgpu::Queue),
    entry: &str,
    inputs: (&Image, Option<&Image>),
    expected: &Image,
    format: wgpu::TextureFormat,
    constants: &[(&str, f64)],
) -> Vec<u8> {
    let (device, queue) = gpu;
    let pipeline = pipeline(device, entry, format, constants);
    let source = upload(device, queue, inputs.0);
    let lower = inputs.1.map(|image| upload(device, queue, image));
    let mut entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&source),
    }];
    if let Some(lower) = &lower {
        entries.push(wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(lower),
        });
    }
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(entry),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    });
    let size = expected.size.map(u32::from);
    let output = crate::color_texture(device, entry, size, format);
    let view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    crate::fullscreen_pass(
        &mut encoder,
        Some(entry),
        wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[&group],
    );
    queue.submit([encoder.finish()]);
    Readback::new(device, FrameConfig::new(size, format, 1), 65536)
        .unwrap()
        .read(device, &crate::test_authority(queue), &output)
        .unwrap()
}

fn fixture(size: [u16; 2]) -> Image {
    let mut pixels = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let v = if x > y { 0.625 } else { 0.125 };
            pixels.push([v, v * 0.5, v * 0.25, 0.75]);
        }
    }
    Image { size, pixels }
}

#[test]
fn gpu_fxaa_matches_scalar_reference_within_the_derived_bounds() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[8, 8], [9, 7], [1, 1]] {
        for amount in [0.0, 0.5, 1.0] {
            for format in [
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ] {
                let mut input = fixture(size);
                if format == wgpu::TextureFormat::Rgba8UnormSrgb {
                    for pixel in &mut input.pixels {
                        let value = if pixel[0] > 0.5 { 0.65625 } else { 0.125 };
                        *pixel = [value, value, value, 0.75];
                    }
                }
                let expected = Image::render(size, |uv| fxaa(&input, uv, amount));
                super::fxaa_tolerance_tests::run_within(
                    (&device, &queue),
                    "fxaa",
                    (&input, None),
                    &expected,
                    format,
                    &[("FXAA", amount.into())],
                );
            }
        }
    }
}

#[test]
fn gpu_bloom_entries_exactly_match_scalar_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    let input = fixture([8, 8]);
    for (threshold, knee) in [(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)] {
        let expected = Image {
            size: input.size,
            pixels: input
                .pixels
                .iter()
                .map(|&c| prefilter(c, threshold, knee))
                .collect(),
        };
        run(
            (&device, &queue),
            "prefilter",
            (&input, None),
            &expected,
            wgpu::TextureFormat::Rgba16Float,
            &[("THRESHOLD", threshold.into()), ("KNEE", knee.into())],
        );
    }
    let down = Image::render([4, 4], |uv| tent(&input, uv));
    run(
        (&device, &queue),
        "downsample",
        (&input, None),
        &down,
        wgpu::TextureFormat::Rgba16Float,
        &[],
    );
    let low = fixture([4, 4]);
    let up = Image::render(input.size, |uv| upsample(&input, &low, uv));
    run(
        (&device, &queue),
        "upsample",
        (&input, Some(&low)),
        &up,
        wgpu::TextureFormat::Rgba16Float,
        &[],
    );
}

fn pipeline(
    device: &wgpu::Device,
    entry: &str,
    format: wgpu::TextureFormat,
    constants: &[(&str, f64)],
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reference filter comparison"),
        source: wgpu::ShaderSource::Wgsl(crate::aces::shader(include_str!("post_filters.wgsl"))),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants,
                ..Default::default()
            },
            targets: &[Some(format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn expected_bytes(expected: &Image, format: wgpu::TextureFormat, entry: &str) -> Vec<u8> {
    expected
        .pixels
        .iter()
        .flat_map(|pixel| {
            pixel.iter().enumerate().flat_map(|(channel, &value)| {
                if format == wgpu::TextureFormat::Rgba16Float {
                    let bits = half_bits(value);
                    assert!(
                        crate::tone_map_gpu_tests::half(&bits.to_le_bytes())
                            .total_cmp(&f64::from(value))
                            .is_eq(),
                        "half fixture must be exactly representable: {value}"
                    );
                    bits.to_le_bytes().to_vec()
                } else {
                    let value = f64::from(value);
                    let encoded = if format == wgpu::TextureFormat::Rgba8UnormSrgb && channel < 3 {
                        if value <= 0.003_130_8 {
                            value * 12.92
                        } else {
                            1.055 * value.powf(1.0 / 2.4) - 0.055
                        }
                    } else {
                        value
                    } * 255.0;
                    assert!(
                        (encoded - (encoded.floor() + 0.5)).abs() > 0.02,
                        "fixture at rounding boundary: {entry}: {encoded}"
                    );
                    vec![
                        (0..=u8::MAX)
                            .find(|&v| f64::from(v).total_cmp(&encoded.round()).is_eq())
                            .unwrap(),
                    ]
                }
            })
        })
        .collect()
}
