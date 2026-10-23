use crate::*;

fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        device,
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        8192,
    )
    .unwrap()
    .read(device, queue, texture)
    .unwrap()
}

fn half(bytes: &[u8]) -> f64 {
    let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let fraction = bits & 1023;
    sign * match exponent {
        0 => f64::from(fraction) * 2.0_f64.powi(-24),
        31 if fraction == 0 => f64::INFINITY,
        31 => f64::NAN,
        _ => (1.0 + f64::from(fraction) / 1024.0) * 2.0_f64.powi(i32::from(exponent) - 15),
    }
}
fn encode(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn encoded_byte(radiance: f64, exposure: f64) -> u8 {
    let encoded = encode(ToneMapOutput::evaluate(radiance, exposure)) * 255.0;
    assert!(
        (encoded - encoded.round()).abs() < 0.1,
        "exact-byte fixture must stay away from sRGB rounding boundaries: {encoded}"
    );
    (0..=u8::MAX)
        .find(|&byte| f64::from(byte).total_cmp(&encoded.round()).is_eq())
        .expect("encoded fixture must fit a byte")
}

#[test]
fn half_decoder_covers_subnormals_and_special_values() {
    for (bits, expected) in [
        (0x0000_u16, 0.0),
        (0x8000, -0.0),
        (0x0001, 2.0_f64.powi(-24)),
        (0x03ff, 1023.0 * 2.0_f64.powi(-24)),
        (0x8001, -2.0_f64.powi(-24)),
        (0x0400, 2.0_f64.powi(-14)),
        (0x7bff, 65504.0),
        (0x7c00, f64::INFINITY),
        (0xfc00, f64::NEG_INFINITY),
    ] {
        assert_eq!(half(&bits.to_le_bytes()).to_bits(), expected.to_bits());
    }
    for bits in [0x7c01_u16, 0x7e00, 0xfe00] {
        assert!(half(&bits.to_le_bytes()).is_nan());
    }
}

#[test]
fn hdr_nonfinite_and_storage_limit_have_finite_output() {
    let (device, queue) = crate::gpu_tests::gpu();
    for samples in [1, 4] {
        for exposure in [1.0, 0.0] {
            nonfinite_output(&device, &queue, samples, exposure);
        }
    }
}

fn nonfinite_output(device: &wgpu::Device, queue: &wgpu::Queue, samples: u32, exposure: f64) {
    let config = FrameConfig::new([3, 1], wgpu::TextureFormat::Rgba16Float, samples);
    let chain = PostChain::new(
        device,
        config,
        Resources::default(),
        ToneMapOutput::new(
            wgpu::TextureFormat::Rgba8UnormSrgb,
            exposure,
            ToneMap::Reinhard,
        )
        .unwrap(),
    )
    .unwrap();
    let output = color_texture(
        device,
        "finite HDR output",
        config.size,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    let pipeline = special_pipeline(device, samples);
    let group = special_group(device, queue, &pipeline);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: chain.targets().scene(),
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
    chain.targets().encode_resolve(&mut encoder);
    if samples == 4 {
        restore_nan(device, queue, &mut encoder, chain.targets().resolved());
    }
    queue.submit([encoder.finish()]);
    let raw = read_texture(device, queue, chain.targets().texture());
    let bits: Vec<_> = raw
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(
        bits, SPECIAL_HALF_WORDS,
        "stored HDR bits, samples={samples}"
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let displayed = read_texture(device, queue, &output);
    assert_nonfinite_output(&displayed, samples, exposure);
}

fn special_pipeline(device: &wgpu::Device, samples: u32) -> wgpu::RenderPipeline {
    test_pipeline(
        device,
        samples,
        r"
@group(0) @binding(0) var values: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(values, vec2<i32>(i32(p.x), 0), 0);
}
",
    )
}

const SPECIAL_HALF_WORDS: [u16; 12] = [
    0x7c00, 0xfc00, 0x7e00, 0x3c00, 0x3a00, 0xbc00, 0x42a0, 0x3c00, 0x7bff, 0x7bff, 0x7bff, 0x3c00,
];

fn special_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
) -> wgpu::BindGroup {
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR special values"),
        size: crate::targets::extent([3, 1]),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let bytes: Vec<_> = SPECIAL_HALF_WORDS
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    queue.write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(24),
            rows_per_image: Some(1),
        },
        source.size(),
    );
    assert_eq!(read_texture(device, queue, &source), bytes);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(
                &source.create_view(&wgpu::TextureViewDescriptor::default()),
            ),
        }],
    })
}

fn restore_nan(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    resolved: &wgpu::TextureView,
) {
    // Metal's MSAA resolve replaces NaN with zero. Repaint only the special-value
    // pixel after resolving so the production output shader actually receives NaN.
    let pipeline = special_pipeline(device, 1);
    let group = special_group(device, queue, &pipeline);
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: resolved,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.set_scissor_rect(0, 0, 1, 1);
    pass.draw(0..3, 0..1);
}

fn assert_nonfinite_output(displayed: &[u8], samples: u32, exposure: f64) {
    let expected = if exposure == 0.0 {
        [[0, 0, 0, 255]; 3]
    } else {
        [
            [255, 0, 0, 255],
            [
                encoded_byte(half(&SPECIAL_HALF_WORDS[4].to_le_bytes()), exposure),
                0,
                encoded_byte(half(&SPECIAL_HALF_WORDS[6].to_le_bytes()), exposure),
                255,
            ],
            [255, 255, 255, 255],
        ]
    };
    for (pixel, expected) in displayed.chunks_exact(4).zip(expected) {
        assert_eq!(pixel, expected, "samples={samples}, exposure={exposure}");
    }
}

#[test]
fn hdr_edge_resolve_fixed_exposure_and_single_srgb_transfer() {
    let (device, queue) = crate::gpu_tests::gpu();
    for samples in [1, 4] {
        let config = FrameConfig::new([17, 17], wgpu::TextureFormat::Rgba16Float, samples);
        let chain = PostChain::new(
            &device,
            config,
            Resources::default(),
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 0.5, ToneMap::Reinhard)
                .unwrap(),
        )
        .unwrap();
        let output = color_texture(
            &device,
            "HDR transfer",
            config.size,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let pipeline = edge_pipeline(&device, samples);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        fullscreen_pass(
            &mut encoder,
            None,
            wgpu::RenderPassColorAttachment {
                view: chain.targets().scene(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            },
            &pipeline,
            &[],
        );
        chain.targets().encode_resolve(&mut encoder);
        chain.encode(
            &mut encoder,
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        queue.submit([encoder.finish()]);
        let raw = read_texture(&device, &queue, chain.targets().texture());
        let displayed = read_texture(&device, &queue, &output);
        assert_pixels(&raw, &displayed, samples);
    }
}

#[test]
fn hdr_resize_failure_preserves_targets_depth_and_output_bindings() {
    let (device, queue) = crate::gpu_tests::gpu();
    let radiance = 0.75;
    let config = FrameConfig::new([5, 3], wgpu::TextureFormat::Rgba16Float, 4);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard).unwrap(),
    )
    .unwrap();
    let old = chain.targets().texture().clone();
    let depth = chain.targets().depth().texture().clone();
    for invalid in [[0, 3], [device.limits().max_texture_dimension_2d + 1, 3]] {
        assert!(chain.resize(&device, invalid).is_err());
        assert_eq!(chain.targets().texture(), &old);
        assert_eq!(chain.targets().depth().texture(), &depth);
    }
    chain.resize(&device, [7, 9]).unwrap();
    assert_eq!(chain.targets().depth().texture().sample_count(), 4);
    assert_eq!(chain.targets().depth().texture().width(), 7);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: chain.targets().scene(),
            resolve_target: chain.targets().resolve(),
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: radiance,
                    g: radiance,
                    b: radiance,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    drop(pass);
    let output = color_texture(
        &device,
        "resize output",
        [7, 9],
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let pixels = Readback::new(&device, FrameConfig::new([7, 9], output.format(), 1), 8192)
        .unwrap()
        .read(&device, &queue, &output)
        .unwrap();
    let expected = encoded_byte(radiance, 1.0);
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [expected, expected, expected, 255])
    );
}

fn edge_pipeline(device: &wgpu::Device, samples: u32) -> wgpu::RenderPipeline {
    test_pipeline(
        device,
        samples,
        r"
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,-1.0), vec2<f32>(-1.0,1.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(4.0, 1.0, 0.25, 1.0); }
",
    )
}

fn test_pipeline(device: &wgpu::Device, samples: u32, source: &str) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::TextureFormat::Rgba16Float.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

fn assert_pixels(raw: &[u8], displayed: &[u8], samples: u32) {
    let mut edges = 0;
    let mut full = 0;
    for (radiance, pixel) in raw.chunks_exact(8).zip(displayed.chunks_exact(4)) {
        let red = half(&radiance[..2]);
        if red > 0.0 && red < 4.0 {
            edges += 1;
        }
        if (red - 4.0).abs() < f64::EPSILON {
            full += 1;
        }
        for channel in 0..3 {
            let expected = encode(ToneMapOutput::evaluate(
                half(&radiance[channel * 2..channel * 2 + 2]),
                0.5,
            )) * 255.0;
            assert!(
                (f64::from(pixel[channel]) - expected).abs() <= 1.1,
                "samples={samples}, actual={pixel:?}, expected={expected}"
            );
        }
    }
    assert!(full > 100, "HDR radiance above one survives until output");
    assert_eq!(
        edges > 10,
        samples == 4,
        "known diagonal coverage must resolve only in MSAA"
    );
}
