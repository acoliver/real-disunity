use super::gaussian_gpu_tests::reference;
use crate::chain::post_effect_tests::{paint, pixels};
use crate::*;
use wgpu::util::DeviceExt;
#[path = "bloom_policy_reference.rs"]
mod scalar;

struct Host {
    bloom: Bloom,
    levels: [GaussianLevel; 8],
    upload: wgpu::Buffer,
    exposure: wgpu::Buffer,
}
impl StageFactory for Host {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set = StageSet::default();
        set.stages.push(Stage {
            id: StageId::new("host exposure", 0),
            label: "host exposure",
            enabled: true,
            operation: Operation::CopyBuffer {
                source: self.upload.clone(),
                destination: self.exposure.clone(),
                bytes: 4,
            },
        });
        let view = BloomPyramid {
            bloom: self.bloom,
            levels: self.levels,
        }
        .append(device, &mut set, targets.resolved(), Some(&self.exposure))?;
        composite(
            device,
            &mut set,
            targets.resolved(),
            &view,
            self.bloom.intensity,
        );
        Ok(set)
    }
}

fn composite(
    device: &wgpu::Device,
    set: &mut StageSet,
    scene: &wgpu::TextureView,
    bloom: &wgpu::TextureView,
    intensity: f32,
) {
    let pipeline = composite_pipeline(device, intensity);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("host composite"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(scene),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(bloom),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new("host composite", 0),
        &pipeline,
        &[&group],
        StageTarget::Output,
    ));
}

fn composite_pipeline(device: &wgpu::Device, intensity: f32) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("host composite"),
        source: wgpu::ShaderSource::Wgsl(
            r"
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var bloom: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
override INTENSITY: f32 = 1.0;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i<<1u)&2u)*2.0-1.0,f32(i&2u)*2.0-1.0,0.0,1.0);
}
@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let rgb = textureLoad(scene, vec2<i32>(p.xy), 0).rgb;
    let uv = p.xy / vec2<f32>(textureDimensions(scene));
    return vec4<f32>(rgb + INTENSITY * textureSampleLevel(bloom,linear_sampler,uv,0.0).rgb,1.0);
}
"
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("host composite"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment"),
            targets: &[Some(wgpu::TextureFormat::Rgba16Float.into())],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("INTENSITY", intensity.into())],
                ..Default::default()
            },
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn host(device: &wgpu::Device, bloom: Bloom, size: [u16; 2]) -> PostChain<Host> {
    host_with_levels(
        device,
        bloom,
        size,
        [GaussianLevel {
            radius: 3,
            tint: [0.5, 1.0, 1.5],
            weight: 0.3,
        }; 8],
    )
}

fn host_with_levels(
    device: &wgpu::Device,
    bloom: Bloom,
    size: [u16; 2],
    levels: [GaussianLevel; 8],
) -> PostChain<Host> {
    let buffer = |usage| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("caller exposure"),
            contents: &0.0_f32.to_le_bytes(),
            usage,
        })
    };
    PostChain::new(
        device,
        FrameConfig::new(size.map(u32::from), wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        Host {
            bloom,
            levels,
            upload: buffer(wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST),
            exposure: buffer(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST),
        },
    )
    .unwrap()
}

fn compare(actual: &[u8], expected: &reference::Image) {
    for (pixel, expected) in actual.chunks_exact(8).zip(&expected.pixels) {
        for (channel, expected) in pixel[..6].chunks_exact(2).zip(expected) {
            let actual = crate::tone_map_gpu_tests::half(channel);
            // Multiple half-float stores and bilinear interpolation accumulate rounding.
            let tolerance = 0.006 * expected.abs() + 0.0002;
            assert!(
                (actual - expected).abs() <= tolerance,
                "{actual} != {expected}, tolerance {tolerance}"
            );
        }
    }
}

fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<Host>,
    size: [u16; 2],
    exposure: f32,
) {
    queue.write_buffer(&chain.factory().upload, 0, &exposure.to_le_bytes());
    let output = color_texture(
        device,
        "caller composite output",
        size.map(u32::from),
        wgpu::TextureFormat::Rgba16Float,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    paint(
        device,
        &mut encoder,
        chain.targets().resolved(),
        "let v=2.0+f32((u32(p.x)*7u+u32(p.y)*3u)%13u)*0.5;return vec4<f32>(v,v*0.25,0.5,1.0);",
    );
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let input = scalar::fixture(size);
    let factory = chain.factory();
    let images = scalar::pyramid(&input, factory.bloom, &factory.levels, f64::from(exposure));
    for (texture, expected) in chain.configured[0].resources.textures.iter().zip(&images) {
        compare(&pixels(device, queue, texture), expected);
    }
    let selected = &images[usize::try_from(factory.bloom.policies.selection.output).unwrap()];
    let expected = reference::Image::new(size, |x, y| {
        let local = input.load(x, y);
        let bloom = selected.scaled(x, y, size);
        std::array::from_fn(|c| local[c] + bloom[c] * f64::from(factory.bloom.intensity))
    });
    compare(&pixels(device, queue, &output), &expected);
}

fn bloom() -> Bloom {
    Bloom {
        threshold: 0.4,
        knee: 0.5,
        intensity: 0.7,
        levels: 3,
        policies: BloomPolicies {
            reduction: BloomReduction::QuarterTexelAfterAverage,
            kernel: GaussianKernelRule::AxisClampedPairs,
            selection: BloomLevelSelection {
                first_blurred: 1,
                output: 1,
            },
            ..BloomPolicies::default()
        },
    }
}

#[test]
fn gpu_bloom_policies_quarter_reduction_level_selection_and_host_composite() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[16, 12], [17, 11], [37, 23]] {
        for tint_pass in [BloomTintPass::Horizontal, BloomTintPass::Vertical] {
            let mut bloom = bloom();
            if size == [37, 23] {
                bloom.levels = 6;
            }
            bloom.policies.tint_pass = tint_pass;
            let chain = host(&device, bloom, size);
            run(&device, &queue, &chain, size, 1.0);
        }
    }
}

#[test]
fn gpu_bloom_policies_observe_caller_exposure_changes_without_rebuild() {
    let (device, queue) = crate::gpu_tests::gpu();
    for curve in [BrightCurve::AcesFilmic, BrightCurve::Reinhard] {
        for reduction in [
            BloomReduction::BoxPerTexel,
            BloomReduction::QuarterTexelAfterAverage,
        ] {
            let mut bloom = bloom();
            bloom.policies.reduction = reduction;
            bloom.policies.extraction = BrightExtraction::ToneCurve(ToneCurveBright {
                curve,
                maximum: 2.0,
                luminance: [0.2126, 0.7152, 0.0722],
                exposure_scale: 0.75,
                contrast: 1.7,
                epsilon: 0.00001,
            });
            let chain = host(&device, bloom, [17, 11]);
            for exposure in [0.0, 0.25, 1.0, 4.0] {
                run(&device, &queue, &chain, [17, 11], exposure);
            }
        }
    }
}

#[test]
fn gpu_bloom_tone_curve_accepts_thresholds_and_contrast_above_64() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut bloom = bloom();
    // A saturating tone curve maps a threshold of 100 near its asymptote. A large exposure
    // scale moves fixture luminance past it, and a large contrast keeps the gate visible.
    bloom.threshold = 100.0;
    bloom.policies.extraction = BrightExtraction::ToneCurve(ToneCurveBright {
        curve: BrightCurve::AcesFilmic,
        maximum: 100.0,
        luminance: [0.2126, 0.7152, 0.0722],
        exposure_scale: 400.0,
        contrast: 10_000.0,
        epsilon: 0.00001,
    });
    let chain = host(&device, bloom, [17, 11]);
    for exposure in [0.25, 1.0, 4.0] {
        run(&device, &queue, &chain, [17, 11], exposure);
    }
}

#[test]
fn gpu_bloom_builder_validates_before_mutating_caller_schedule() {
    let (device, _) = crate::gpu_tests::gpu();
    let texture = color_texture(
        &device,
        "caller scene",
        [17, 11],
        wgpu::TextureFormat::Rgba16Float,
    );
    let scene = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut set = StageSet::default();
    let mut bloom = bloom();
    bloom.policies.extraction = BrightExtraction::ToneCurve(ToneCurveBright {
        curve: BrightCurve::AcesFilmic,
        maximum: 32.0,
        luminance: [0.2126, 0.7152, 0.0722],
        exposure_scale: 1.0,
        contrast: 1.0,
        epsilon: 0.00001,
    });
    let builder = BloomPyramid {
        bloom,
        levels: [GaussianLevel {
            radius: 3,
            tint: [1.0; 3],
            weight: 1.0,
        }; 8],
    };
    assert!(builder.append(&device, &mut set, &scene, None).is_err());
    let wrong_usage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("caller uniform"),
        contents: &1.0_f32.to_le_bytes(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    assert!(
        builder
            .append(&device, &mut set, &scene, Some(&wrong_usage))
            .is_err()
    );
    assert!(set.stages.is_empty());
    assert!(set.resources.textures.is_empty());
    assert!(set.resources.buffers.is_empty());
}

#[test]
fn gpu_bloom_default_builder_preserves_public_stage_identities() {
    let (device, _) = crate::gpu_tests::gpu();
    let texture = color_texture(
        &device,
        "caller scene",
        [17, 11],
        wgpu::TextureFormat::Rgba16Float,
    );
    let scene = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut set = StageSet::default();
    let mut bloom = bloom();
    bloom.policies = BloomPolicies::default();
    BloomPyramid {
        bloom,
        levels: [GaussianLevel {
            radius: 3,
            tint: [1.0; 3],
            weight: 1.0,
        }; 8],
    }
    .append(&device, &mut set, &scene, None)
    .unwrap();
    let mut expected = vec![StageId::new("gaussian_bright", 0)];
    expected.extend((1..bloom.levels).map(|i| StageId::new("gaussian_downsample", i)));
    for i in (0..bloom.levels).rev() {
        expected.push(StageId::new("gaussian_horizontal", i));
        expected.push(StageId::new("gaussian_vertical", i));
    }
    assert_eq!(
        set.stages.iter().map(|stage| stage.id).collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn gpu_bloom_stock_hdr_uses_fixed_and_adaptive_exposure_inputs() {
    let (device, queue) = crate::gpu_tests::gpu();
    for adaptive in [false, true] {
        let mut bloom = bloom();
        bloom.policies.extraction = BrightExtraction::ToneCurve(ToneCurveBright {
            curve: BrightCurve::AcesFilmic,
            maximum: 2.0,
            luminance: [0.2126, 0.7152, 0.0722],
            exposure_scale: 0.75,
            contrast: 1.7,
            epsilon: 0.00001,
        });
        let levels = [GaussianLevel {
            radius: 3,
            tint: [0.5, 1.0, 1.5],
            weight: 0.3,
        }; 8];
        let settings = HdrPostSettings {
            bloom: Some(bloom),
            bloom_kernel: BloomKernel::SeparablePyramid(levels),
            adaptive: adaptive.then(|| AdaptiveExposure {
                fixed_exposure: Some(2.0),
                ..crate::chain::post_effect_tests::adaptive()
            }),
            ..HdrPostSettings::default()
        };
        let factory =
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 2.0, ToneMap::Reinhard)
                .unwrap()
                .with_effects(&device, settings)
                .unwrap();
        let chain = PostChain::new(
            &device,
            FrameConfig::new([17, 11], wgpu::TextureFormat::Rgba16Float, 1),
            Resources::default(),
            factory,
        )
        .unwrap();
        crate::chain::post_effect_tests::step(
            &device,
            &queue,
            &chain,
            0,
            "let v=2.0+f32((u32(p.x)*7u+u32(p.y)*3u)%13u)*0.5;return vec4<f32>(v,v*0.25,0.5,1.0);",
            true,
        );
        let expected = scalar::pyramid(&scalar::fixture([17, 11]), bloom, &levels, 2.0);
        for (texture, expected) in chain.configured[0].resources.textures.iter().zip(&expected) {
            compare(&pixels(&device, &queue, texture), expected);
        }
    }
}

fn terminal_levels() -> [GaussianLevel; 8] {
    std::array::from_fn(|index| {
        let i = f32::from(u16::try_from(index).unwrap());
        GaussianLevel {
            radius: 44,
            tint: [0.5 + i * 0.125, 1.5 - i * 0.125, 0.75 + i * 0.25],
            weight: 0.125 + i * 0.03125,
        }
    })
}

#[test]
fn gpu_bloom_repeat_terminal_builder_matches_cpu() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[1, 1], [17, 9], [32, 32]] {
        for reduction in [
            BloomReduction::BoxPerTexel,
            BloomReduction::QuarterTexelAfterAverage,
        ] {
            for tint_pass in [BloomTintPass::Horizontal, BloomTintPass::Vertical] {
                for (first_blurred, output) in [(0, 0), (1, 1), (4, 5)] {
                    let mut bloom = bloom();
                    bloom.levels = 6;
                    bloom.policies.terminal = BloomTerminal::Repeat;
                    bloom.policies.reduction = reduction;
                    bloom.policies.tint_pass = tint_pass;
                    bloom.policies.selection = BloomLevelSelection {
                        first_blurred,
                        output,
                    };
                    let chain = host_with_levels(&device, bloom, size, terminal_levels());
                    run(&device, &queue, &chain, size, 1.0);
                }
            }
        }
    }
}

#[test]
fn gpu_bloom_repeat_terminal_stock_matches_cpu() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[1, 1], [17, 9], [32, 32]] {
        let mut bloom = bloom();
        bloom.levels = 6;
        bloom.policies.terminal = BloomTerminal::Repeat;
        let levels = terminal_levels();
        let settings = HdrPostSettings {
            bloom: Some(bloom),
            bloom_kernel: BloomKernel::SeparablePyramid(levels),
            ..HdrPostSettings::default()
        };
        let chain = crate::chain::post_effect_tests::chain(&device, &settings, size.map(u32::from));
        crate::chain::post_effect_tests::step(
            &device,
            &queue,
            &chain,
            0,
            "let v=2.0+f32((u32(p.x)*7u+u32(p.y)*3u)%13u)*0.5;return vec4<f32>(v,v*0.25,0.5,1.0);",
            false,
        );
        let expected = scalar::pyramid(&scalar::fixture(size), bloom, &levels, 1.0);
        for (texture, expected) in chain.configured[0].resources.textures.iter().zip(&expected) {
            compare(&pixels(&device, &queue, texture), expected);
        }
    }
}

#[test]
fn gpu_bloom_default_builder_rejects_repeated_terminal_levels() {
    let (device, _) = crate::gpu_tests::gpu();
    for size in [[1, 1], [17, 9], [32, 32]] {
        let texture = color_texture(
            &device,
            "small scene",
            size,
            wgpu::TextureFormat::Rgba16Float,
        );
        let source = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut bloom = bloom();
        bloom.levels = 6;
        let mut set = StageSet::default();
        let result = BloomPyramid {
            bloom,
            levels: terminal_levels(),
        }
        .append(&device, &mut set, &source, None);
        assert!(matches!(
            result,
            Err(PostError::UnsupportedProfile(
                "Gaussian bloom levels exceed extent"
            ))
        ));
        assert!(set.stages.is_empty());
        assert!(set.resources.textures.is_empty());
    }
}
