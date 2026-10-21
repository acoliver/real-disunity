use super::*;

#[test]
fn invalid_frame_targets_fail_without_gpu_validation_errors() {
    let (device, _) = crate::gpu_tests::gpu();
    for (format, samples, expected) in [
        (
            wgpu::TextureFormat::Depth32Float,
            1,
            PostError::NonColorFormat(wgpu::TextureFormat::Depth32Float),
        ),
        (
            wgpu::TextureFormat::Rgba8Uint,
            4,
            PostError::UnsupportedResolve(wgpu::TextureFormat::Rgba8Uint),
        ),
        (
            wgpu::TextureFormat::R32Float,
            4,
            PostError::UnsupportedResolve(wgpu::TextureFormat::R32Float),
        ),
    ] {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = FrameTargets::new(&device, FrameConfig::new([8, 8], format, samples));
        let validation = pollster::block_on(scope.pop());
        assert_eq!(result.err(), Some(expected));
        assert!(
            validation.is_none(),
            "unexpected GPU validation: {validation:?}"
        );
    }
}

#[test]
fn copy_texture_load_preserves_float32_without_requiring_filterability() {
    for optional in [wgpu::Features::empty(), wgpu::Features::FLOAT32_FILTERABLE] {
        let (device, queue) = crate::gpu_tests::gpu_with_optional_features(optional);
        for format in [
            wgpu::TextureFormat::R32Float,
            wgpu::TextureFormat::Rgba32Float,
        ] {
            assert_eq!(
                format.sample_type(None, Some(device.features())),
                Some(wgpu::TextureSampleType::Float {
                    filterable: device
                        .features()
                        .contains(wgpu::Features::FLOAT32_FILTERABLE),
                }),
            );
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let config = FrameConfig::new([3, 2], format, 1);
            let chain = PostChain::new(
                &device,
                config,
                Resources::default(),
                CopyOutput::new(&device, format),
            )
            .unwrap();
            let output = color_texture(&device, "float32 copy", config.size, format);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            clear_hdr(&mut encoder, chain.targets());
            chain.encode(
                &mut encoder,
                &output.create_view(&wgpu::TextureViewDescriptor::default()),
            );
            queue.submit([encoder.finish()]);
            let mut readback = Readback::new(&device, config, 1024).unwrap();
            let original = readback
                .read(&device, &queue, chain.targets().texture())
                .unwrap();
            let copied = readback.read(&device, &queue, &output).unwrap();
            let validation = pollster::block_on(scope.pop());
            assert_eq!(&original[..4], &2.5_f32.to_le_bytes());
            assert_eq!(
                copied, original,
                "float32 source/output stay unclamped and exact"
            );
            assert!(
                validation.is_none(),
                "unexpected GPU validation: {validation:?}"
            );
        }
    }
}

#[test]
fn hdr_four_sample_resolve_and_copy_preserve_values_outside_unorm_range() {
    let (device, queue) = crate::gpu_tests::gpu();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let format = wgpu::TextureFormat::Rgba16Float;
    let factory = CopyOutput::new(&device, format);
    let mut chain = PostChain::new(
        &device,
        FrameConfig::new([17, 9], format, 4),
        Resources::default(),
        factory.clone(),
    )
    .unwrap();
    for size in [[17, 9], [5, 3]] {
        chain.resize(&device, size).unwrap();
        chain.reconfigure(&device, factory.clone()).unwrap();
        let output = color_texture(&device, "HDR resolve copy", size, format);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        clear_hdr(&mut encoder, chain.targets());
        chain.encode(
            &mut encoder,
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        queue.submit([encoder.finish()]);
        let mut readback = Readback::new(&device, FrameConfig::new(size, format, 1), 4096).unwrap();
        let pixels = readback.read(&device, &queue, &output).unwrap();
        assert_eq!(
            pixels.len(),
            usize::try_from(size[0] * size[1] * 8).unwrap()
        );
        for pixel in pixels.chunks_exact(8) {
            assert_eq!(pixel, [0x00, 0x41, 0x00, 0xb8, 0x00, 0x34, 0x00, 0x38]);
        }
    }
    let validation = pollster::block_on(scope.pop());
    assert!(
        validation.is_none(),
        "unexpected GPU validation: {validation:?}"
    );
}

#[test]
fn copy_rejects_integer_sources_without_gpu_validation_errors() {
    let (device, _) = crate::gpu_tests::gpu();
    for format in [
        wgpu::TextureFormat::Rgba8Uint,
        wgpu::TextureFormat::Rgba8Sint,
    ] {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let targets = FrameTargets::new(&device, FrameConfig::new([8, 8], format, 1)).unwrap();
        assert!(targets.resolve().is_none());
        let result = CopyOutput::new(&device, wgpu::TextureFormat::Rgba8Unorm).build(
            &device,
            &targets,
            &Resources::default(),
        );
        let validation = pollster::block_on(scope.pop());
        assert_eq!(result.err(), Some(PostError::CopySourceFormat(format)));
        assert!(
            validation.is_none(),
            "unexpected GPU validation: {validation:?}"
        );
    }
}

#[test]
fn copy_rejects_non_float_or_non_renderable_outputs_without_gpu_validation_errors() {
    let (device, _) = crate::gpu_tests::gpu();
    for format in [
        wgpu::TextureFormat::Rgba8Uint,
        wgpu::TextureFormat::Rgba8Sint,
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Stencil8,
        wgpu::TextureFormat::Rgba8Snorm,
        wgpu::TextureFormat::Rg11b10Ufloat,
        wgpu::TextureFormat::Bc1RgbaUnorm,
    ] {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = PostChain::new(
            &device,
            FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1),
            Resources::default(),
            CopyOutput::new(&device, format),
        );
        let validation = pollster::block_on(scope.pop());
        assert_eq!(result.err(), Some(PostError::CopyOutputFormat(format)));
        assert!(
            validation.is_none(),
            "unexpected GPU validation: {validation:?}"
        );
    }
}

#[test]
fn copy_preserves_pattern_orientation_alpha_and_every_srgb_code() {
    let (device, queue) = crate::gpu_tests::gpu();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let config = FrameConfig::new([257, 3], format, 1);
        let chain = PostChain::new(
            &device,
            config,
            Resources::default(),
            CopyOutput::new(&device, format),
        )
        .unwrap();
        let pipeline = pattern_pipeline(&device, format);
        let output = color_texture(&device, "exact copy probe", config.size, format);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        fullscreen_pass(
            &mut encoder,
            Some("pattern"),
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
            &[],
        );
        chain.encode(
            &mut encoder,
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        queue.submit([encoder.finish()]);
        let mut readback = Readback::new(&device, config, 1024 * 1024).unwrap();
        let original = readback
            .read(&device, &queue, chain.targets().texture())
            .unwrap();
        let copied = readback.read(&device, &queue, &output).unwrap();
        assert_ne!(&original[..4], &original[4..8]);
        assert_ne!(&original[..4], &original[257 * 4..258 * 4]);
        let red: std::collections::BTreeSet<_> =
            original.chunks_exact(4).map(|pixel| pixel[0]).collect();
        assert_eq!(red.len(), 256, "pattern exercises every encoded value");
        assert_eq!(
            copied, original,
            "no orientation, alpha or transfer-function changes"
        );
    }
}

fn pattern_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("encoded color pattern"), source: wgpu::ShaderSource::Wgsl(r"
override SRGB: bool = false;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn linear(code: f32) -> f32 {
    let v = code / 255.0;
    return select(pow((v + 0.055) / 1.055, 2.4), v / 12.92, v <= 0.04045);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let x = u32(p.x); let y = u32(p.y);
    let codes = vec3<f32>(f32(x % 256u), f32(y * 71u), f32((x * 17u) % 256u));
    return vec4<f32>(select(codes / 255.0, vec3<f32>(linear(codes.r), linear(codes.g), linear(codes.b)), SRGB), f32((x * 23u) % 256u) / 255.0);
}
".into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    fullscreen_pipeline(
        device,
        FullscreenDescriptor {
            label: None,
            layout: &layout,
            module: &module,
            vertex_entry: "vs",
            fragment_entry: "fs",
            format,
            constants: &[("SRGB", if format.is_srgb() { 1.0 } else { 0.0 })],
        },
    )
}

fn clear_hdr(encoder: &mut wgpu::CommandEncoder, targets: &FrameTargets) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("HDR copy source"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: targets.scene(),
            depth_slice: None,
            resolve_target: targets.resolve(),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 2.5,
                    g: -0.5,
                    b: 0.25,
                    a: 0.5,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..wgpu::RenderPassDescriptor::default()
    });
}
