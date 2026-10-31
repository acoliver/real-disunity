use crate::upscale_tests::{fixture, paint};
use crate::*;
use worldrunner_render_plan::post::RenderScale;

fn factory(device: &wgpu::Device, sharpness: Option<f32>) -> ToneMapOutput {
    ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .with_effects(
            device,
            HdrPostSettings {
                upscale: Some(UpscaleSettings {
                    mode: UpscaleMode::Easu,
                    sharpness,
                }),
                ..HdrPostSettings::default()
            },
        )
        .unwrap()
}

#[test]
fn gpu_hdr_upscale_runs_after_tone_mapping_before_transfer() {
    let (device, queue) = crate::gpu_tests::gpu();
    let config = FrameConfig::new([50, 26], wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent([33, 17]);
    for sharpness in [None, Some(0.0)] {
        let chain = PostChain::new(
            &device,
            config,
            Resources::default(),
            factory(&device, sharpness),
        )
        .unwrap();
        paint(&device, &queue, chain.targets().texture(), 2);
        let mut mapped = fixture([33, 17], 2);
        for p in &mut mapped.pixels {
            for c in &mut p[..3] {
                *c = ToneMapOutput::evaluate(*c, 1.0);
            }
        }
        let expected = mapped.reconstruct([50, 26], true);
        let expected =
            sharpness.map_or_else(|| expected.clone(), |s| expected.sharpen(f64::from(s)));
        let actual = capture(&device, &queue, &chain);
        for (a, e) in actual.chunks_exact(4).zip(expected.pixels) {
            for c in 0..4 {
                let encoded = if c == 3 { e[c] } else { srgb(e[c]) };
                // The sRGB attachment's quantization is permitted one 8-bit code step.
                assert!((f64::from(a[c]) - encoded * 255.0).abs() <= 1.0);
            }
        }
    }
}
fn srgb(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}
fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<ToneMapOutput>,
) -> Vec<u8> {
    let config = FrameConfig::new(
        chain.targets().config().output_extent(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
    );
    let output = color_texture(
        device,
        "HDR reconstruction output",
        config.size,
        config.format,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    Readback::new(device, config, 100_000)
        .unwrap()
        .read(device, &crate::test_authority(queue), &output)
        .unwrap()
}

#[test]
fn gpu_native_extent_omits_reconstruction_and_sharpening() {
    let (device, queue) = crate::gpu_tests::gpu();
    let config = FrameConfig::new([33, 17], wgpu::TextureFormat::Rgba16Float, 1);
    let plain =
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard).unwrap();
    let control = PostChain::new(&device, config, Resources::default(), plain).unwrap();
    let candidate = PostChain::new(
        &device,
        config,
        Resources::default(),
        factory(&device, Some(0.0)),
    )
    .unwrap();
    paint(&device, &queue, control.targets().texture(), 3);
    paint(&device, &queue, candidate.targets().texture(), 3);
    assert_eq!(
        capture(&device, &queue, &candidate),
        capture(&device, &queue, &control)
    );
    // The public schedule contract must preserve native pass count.
    let schedule = candidate
        .factory()
        .build(&device, candidate.targets(), &Resources::default())
        .unwrap();
    assert_eq!(schedule.stages.iter().filter(|s| s.enabled).count(), 1);
}

#[test]
fn gpu_post_chain_scale_timings_are_informational() {
    let (device, queue) =
        crate::gpu_tests::gpu_with_optional_features(wgpu::Features::TIMESTAMP_QUERY);
    let authority = crate::test_authority(&queue);
    for millionths in [500_000, 770_000, 1_000_000] {
        let output = [320, 180];
        let render = RenderScale::new(millionths)
            .unwrap()
            .extent(output)
            .unwrap();
        let config = FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let chain = PostChain::new(
            &device,
            config,
            Resources::default(),
            factory(&device, Some(0.0)),
        )
        .unwrap();
        chain
            .configure_timing(&device, &queue, TimingMode::Required)
            .unwrap();
        paint(&device, &queue, chain.targets().texture(), 2);
        let target = color_texture(
            &device,
            "timing output",
            output,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        for frame in 0..3 {
            let mut encoder = authority.begin(&device).unwrap();
            chain.begin_timing(frame).unwrap();
            chain.encode(&mut encoder, &view);
            chain.finish_timing(&mut encoder).unwrap();
            let submission = encoder.finish().submit().unwrap();
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
            let timings = chain.take_timings().unwrap();
            assert_eq!(timings.len(), 1);
            let total_ns: f64 = timings[0]
                .passes
                .iter()
                .map(|p| p.duration_ns.expect("valid timestamp interval"))
                .sum();
            eprintln!(
                "post scale={millionths}/1000000 render={render:?} output={output:?} frame={frame} GPU={:.3} ms",
                total_ns / 1_000_000.0
            );
        }
    }
}

#[test]
fn gpu_hdr_reduced_extent_requires_explicit_reconstruction() {
    let (device, _) = crate::gpu_tests::gpu();
    let config = FrameConfig::new([50, 26], wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent([33, 17]);
    let plain =
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard).unwrap();
    assert!(matches!(
        PostChain::new(&device, config, Resources::default(), plain),
        Err(PostError::UnsupportedProfile(_))
    ));
}
