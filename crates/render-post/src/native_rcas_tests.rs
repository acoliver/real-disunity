use crate::upscale_tests::{fixture, paint};
use crate::*;

#[test]
fn gpu_native_rcas_follows_resolve_and_cas() {
    let (device, queue) = crate::gpu_tests::gpu();
    for resolve in [
        PostResolve::fxaa(1.0),
        PostResolve::smaa(SmaaPreset::Low),
        PostResolve::cmaa2(Cmaa2Preset::High),
    ] {
        let settings = HdrPostSettings {
            resolve,
            sharpen: Some(CasSettings::new(0.5)),
            native_rcas: Some(NativeRcasSettings {
                stops: 0.75,
                domain: RcasDomain::EncodedOutput,
            }),
            ..HdrPostSettings::default()
        };
        let config = FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba16Float, 1);
        let chain = PostChain::new(
            &device,
            config,
            Resources::default(),
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
                .unwrap()
                .with_effects(&device, settings)
                .unwrap(),
        )
        .unwrap();
        paint(&device, &queue, chain.targets().texture(), 1);
        let image = capture(&device, &queue, &chain);
        assert_edge(&image);
        assert!(image.chunks_exact(4).all(|pixel| pixel[3] == 191));
        let schedule = chain
            .factory()
            .build(&device, chain.targets(), &Resources::default())
            .unwrap();
        let names: Vec<_> = schedule.stages.iter().map(|s| s.id.name).collect();
        assert_eq!(&names[names.len() - 2..], ["cas", "rcas"]);
    }
}

#[test]
fn gpu_native_rcas_setting_does_not_change_upscaled_bytes() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut results = Vec::new();
    for native_rcas in [
        None,
        Some(NativeRcasSettings {
            stops: 0.75,
            domain: RcasDomain::EncodedOutput,
        }),
    ] {
        let settings = HdrPostSettings {
            native_rcas,
            upscale: Some(UpscaleSettings {
                mode: UpscaleMode::Easu,
                sharpness: Some(0.75),
            }),
            ..HdrPostSettings::default()
        };
        let config = FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent([4, 2]);
        let chain = PostChain::new(
            &device,
            config,
            Resources::default(),
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
                .unwrap()
                .with_effects(&device, settings)
                .unwrap(),
        )
        .unwrap();
        paint(&device, &queue, chain.targets().texture(), 1);
        results.push(capture(&device, &queue, &chain));
        let schedule = chain
            .factory()
            .build(&device, chain.targets(), &Resources::default())
            .unwrap();
        assert_eq!(
            schedule
                .stages
                .iter()
                .map(|s| s.id.name)
                .collect::<Vec<_>>(),
            ["tone", "easu", "rcas"]
        );
    }
    assert_edge(&results[0]);
    assert_eq!(results[0], results[1]);
}

#[test]
fn native_rcas_allocation_counts_one_f32_texture_per_slot() {
    let config = FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba16Float, 1);
    let off = HdrPostSettings {
        tone_map: Some(ToneMap::Reinhard),
        ..HdrPostSettings::default()
    };
    let on = HdrPostSettings {
        native_rcas: Some(NativeRcasSettings {
            stops: 0.75,
            domain: RcasDomain::EncodedOutput,
        }),
        ..off
    };
    let bytes = |settings| {
        crate::allocation::post_allocations(config, settings)
            .validate(&wgpu::Limits::default())
            .unwrap()
    };
    assert_eq!(bytes(on) - bytes(off), 7 * 3 * 16 * 3);
}

#[derive(Clone)]
struct NativeOnly(NativeRcasSettings);
impl StageFactory for NativeOnly {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set = StageSet::default();
        crate::upscale::append_native(
            device,
            &mut set,
            targets.resolved(),
            self.0,
            wgpu::TextureFormat::Rgba32Float,
        );
        Ok(set)
    }
}

#[test]
fn gpu_native_rcas_linear_kernel_matches_f64_without_quantization() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[1, 1], [3, 1], [7, 3]] {
        for pattern in [0, 1, 4] {
            for domain in [RcasDomain::DisplayLinear, RcasDomain::EncodedOutput] {
                verify_linear(&device, &queue, size, pattern, domain);
            }
        }
    }
}
fn verify_linear(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: [u32; 2],
    pattern: u32,
    domain: RcasDomain,
) {
    let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba32Float, 1);
    let chain = PostChain::new(
        device,
        config,
        Resources::default(),
        NativeOnly(NativeRcasSettings {
            stops: 0.75,
            domain,
        }),
    )
    .unwrap();
    paint(device, queue, chain.targets().texture(), pattern);
    let output = color_texture(device, "linear native RCAS", config.size, config.format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let bytes = Readback::new(device, config, 4096)
        .unwrap()
        .read(device, &crate::test_authority(queue), &output)
        .unwrap();
    let expected = fixture(size.map(|v| i32::try_from(v).unwrap()), pattern).sharpen(0.75);
    for (actual, expected) in bytes.chunks_exact(16).zip(expected.pixels) {
        for (channel, expected) in expected.into_iter().enumerate() {
            let actual =
                f32::from_le_bytes(actual[channel * 4..channel * 4 + 4].try_into().unwrap());
            assert!((f64::from(actual) - expected).abs() <= 0.000_01);
        }
    }
}

#[test]
fn native_rcas_rejects_invalid_stops() {
    for stops in [-1.0, 2.01, f32::NAN, f32::INFINITY] {
        let settings = HdrPostSettings {
            native_rcas: Some(NativeRcasSettings {
                stops,
                domain: RcasDomain::EncodedOutput,
            }),
            ..HdrPostSettings::default()
        };
        assert!(settings.validate().is_err());
    }
    for stops in [0.0, 0.75, 2.0] {
        assert!(
            NativeRcasSettings {
                stops,
                domain: RcasDomain::DisplayLinear
            }
            .validate()
            .is_ok()
        );
    }
}

fn chain(
    device: &wgpu::Device,
    native_rcas: Option<NativeRcasSettings>,
) -> PostChain<ToneMapOutput> {
    PostChain::new(
        device,
        FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
            .unwrap()
            .with_effects(
                device,
                HdrPostSettings {
                    native_rcas,
                    ..HdrPostSettings::default()
                },
            )
            .unwrap(),
    )
    .unwrap()
}

fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<ToneMapOutput>,
) -> Vec<u8> {
    let config = FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba8UnormSrgb, 1);
    let output = color_texture(device, "native RCAS test", config.size, config.format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    Readback::new(device, config, 4096)
        .unwrap()
        .read(device, &crate::test_authority(queue), &output)
        .unwrap()
}

fn srgb(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[test]
fn gpu_native_rcas_domains_match_independent_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    let baseline = chain(&device, None);
    paint(&device, &queue, baseline.targets().texture(), 1);
    let baseline = capture(&device, &queue, &baseline);
    assert_edge(&baseline);
    let mut results = Vec::new();
    for domain in [RcasDomain::DisplayLinear, RcasDomain::EncodedOutput] {
        let chain = chain(
            &device,
            Some(NativeRcasSettings {
                stops: 0.75,
                domain,
            }),
        );
        paint(&device, &queue, chain.targets().texture(), 1);
        let mut expected = fixture([7, 3], 1);
        for pixel in &mut expected.pixels {
            for value in &mut pixel[..3] {
                *value = ToneMapOutput::evaluate(*value, 1.0);
                if domain == RcasDomain::EncodedOutput {
                    *value = srgb(*value);
                }
            }
        }
        expected = expected.sharpen(0.75);
        let actual = capture(&device, &queue, &chain);
        assert!(actual[9 * 4] < baseline[9 * 4]);
        assert!(actual[10 * 4] > baseline[10 * 4]);
        for (a, e) in actual.chunks_exact(4).zip(expected.pixels) {
            for c in 0..4 {
                let value = if c == 3 || domain == RcasDomain::EncodedOutput {
                    e[c]
                } else {
                    srgb(e[c])
                };
                assert!(
                    (f64::from(a[c]) - value.clamp(0.0, 1.0) * 255.0).abs() <= 1.0,
                    "{domain:?} channel {c}: {} != {value}",
                    a[c]
                );
            }
        }
        let schedule = chain
            .factory()
            .build(&device, chain.targets(), &Resources::default())
            .unwrap();
        assert_eq!(
            schedule
                .stages
                .iter()
                .map(|s| s.id.name)
                .collect::<Vec<_>>(),
            ["tone", "rcas"]
        );
        results.push(actual);
    }
    assert_ne!(results[0], results[1]);
}

#[test]
fn gpu_native_rcas_default_is_byte_identical() {
    let (device, queue) = crate::gpu_tests::gpu();
    let candidate = chain(&device, None);
    let control = PostChain::new(
        &device,
        candidate.targets().config(),
        Resources::default(),
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard).unwrap(),
    )
    .unwrap();
    for chain in [&candidate, &control] {
        paint(&device, &queue, chain.targets().texture(), 1);
    }
    let actual = capture(&device, &queue, &candidate);
    assert_edge(&actual);
    assert_eq!(actual, capture(&device, &queue, &control));
    assert_eq!(HdrPostSettings::default().native_rcas, None);
}

fn assert_edge(image: &[u8]) {
    assert_eq!(image.len(), 7 * 3 * 4);
    for y in 0..3 {
        let dark = image[(y * 7 + 2) * 4];
        let light = image[(y * 7 + 3) * 4];
        assert!(dark > 20 && light < 235);
        assert!(i16::from(light) - i16::from(dark) > 40);
    }
}
