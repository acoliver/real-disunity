use crate::exact_tests::{nearest_unit_f32, reference_alpha8, reference_srgb8, threshold};
use crate::{CopyOutput, ExactSrgb8, FrameConfig, FrameTargets, PostChain, Readback, Resources};

#[test]
fn exact_profile_rejects_missing_features_and_quantized_working_attachments() {
    let (device, _) = crate::gpu_tests::gpu();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    assert_eq!(
        ExactSrgb8::frame_config(&device, [8, 8]),
        Err(crate::PostError::ExactCompositionFeatures)
    );
    assert_eq!(
        ExactSrgb8::new(&device, [8, 8]).err(),
        Some(crate::PostError::ExactCompositionFeatures)
    );
    let config = FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8UnormSrgb, 1);
    assert_eq!(
        PostChain::new(
            &device,
            config,
            Resources::default(),
            CopyOutput::exact_srgb8(&device)
        )
        .err(),
        Some(crate::PostError::ExactCompositionFeatures)
    );
    assert!(pollster::block_on(scope.pop()).is_none());
    let (device, _) = crate::gpu_tests::gpu_with_optional_features(ExactSrgb8::REQUIRED_FEATURES);
    assert!(device.features().contains(ExactSrgb8::REQUIRED_FEATURES));
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    assert_eq!(
        PostChain::new(
            &device,
            config,
            Resources::default(),
            CopyOutput::exact_srgb8(&device)
        )
        .err(),
        Some(crate::PostError::ExactCompositionFormat)
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn exact_srgb8_matches_f64_oracle_at_every_threshold_neighbor() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: true,
        ..Default::default()
    }))
    .expect("contained software GPU test requires an adapter");
    assert!(adapter.features().contains(ExactSrgb8::REQUIRED_FEATURES));
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: ExactSrgb8::REQUIRED_FEATURES,
        ..Default::default()
    }))
    .unwrap();

    let values = threshold_values();
    let width = u32::try_from(values.len()).unwrap();
    let size = [width, 1];
    let config = FrameConfig::new(size, ExactSrgb8::WORKING_FORMAT, 1);
    let chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::exact_srgb8(&device),
    )
    .unwrap();
    let output = ExactSrgb8::new(&device, size).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    render_values(&device, &mut encoder, chain.targets(), &values);
    chain.encode(&mut encoder, output.write_view());
    queue.submit([encoder.finish()]);
    let mut readback = Readback::new(
        &device,
        FrameConfig::new(size, ExactSrgb8::OUTPUT_FORMAT, 1),
        u64::from(width) * 4 + 256,
    )
    .unwrap();
    let actual = readback.read(&device, &queue, output.texture()).unwrap();
    assert_eq!(actual.len(), values.len() * 4);
    for (index, (value, pixel)) in values.iter().zip(actual.chunks_exact(4)).enumerate() {
        let expected = reference_srgb8(*value);
        let alpha = reference_alpha8(*value);
        assert_eq!(
            pixel,
            [expected, expected, expected, alpha],
            "sample {index}: {value:?}"
        );
    }
    let observed: std::collections::BTreeSet<_> =
        actual.chunks_exact(4).map(|pixel| pixel[0]).collect();
    assert_eq!(observed.len(), 256, "all output codes are exercised");
}

fn threshold_values() -> Vec<f32> {
    let mut values = vec![
        0.0_f32,
        -0.0,
        0.5,
        0.25,
        0.125,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
        -1.0,
        1.0,
    ];
    for k in 0..255 {
        for boundary in [threshold(k), nearest_unit_f32((f64::from(k) + 0.5) / 255.0)] {
            let bits = boundary.to_bits();
            values.extend([f32::from_bits(bits - 1), boundary, f32::from_bits(bits + 1)]);
        }
    }
    values
}

fn render_values(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    targets: &FrameTargets,
    values: &[f32],
) {
    use wgpu::util::DeviceExt;

    let data: Vec<u8> = values
        .iter()
        .flat_map(|value| [*value; 4].into_iter().flat_map(f32::to_le_bytes))
        .collect();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("exact sRGB test values"),
        contents: &data,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let pipeline = values_pipeline(device);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("exact sRGB test values"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("exact sRGB test input"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: targets.scene(),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.draw(0..3, 0..1);
}

fn values_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("exact sRGB test source"),
        source: wgpu::ShaderSource::Wgsl(
            r"
@group(0) @binding(0) var<storage, read> values: array<vec4<f32>>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return values[u32(p.x)];
}
"
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("exact sRGB test source"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(ExactSrgb8::WORKING_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}
