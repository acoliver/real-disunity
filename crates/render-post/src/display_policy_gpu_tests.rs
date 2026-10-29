use super::{output, paint, pixels};
use crate::*;

fn encoded(x: f64) -> f64 {
    let v = if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    v.clamp(0.0, 1.0) * 255.0
}

fn render(
    gpu: (&wgpu::Device, &wgpu::Queue),
    config: FrameConfig,
    display: (ToneMap, f64),
    settings: &HdrPostSettings,
    body: &str,
) -> (Vec<u8>, Vec<StageId>) {
    let (device, queue) = gpu;
    let factory = ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, display.1, display.0)
        .unwrap()
        .with_effects(device, *settings)
        .unwrap();
    let chain = PostChain::new(device, config, Resources::default(), factory).unwrap();
    let target = output(device, config.size);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    // Paint the post-resolve input directly, independent of implementation-defined MSAA positions.
    paint(device, &mut encoder, chain.targets().resolved(), body);
    chain
        .factory()
        .prepare_effects(&mut encoder, 0, 0.0, true)
        .unwrap();
    chain.encode(
        &mut encoder,
        &target.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let ids = chain.configured[0].stages.iter().map(|s| s.id).collect();
    (pixels(device, queue, &target), ids)
}

#[test]
fn filmic_post_resolve_hdr_ramp_transfer_alpha_and_feature_off_ids() {
    let (device, queue) = crate::gpu_tests::gpu();
    let values = [0.0, 0.03125, 0.125, 0.5, 2.0, 4.0, 16.0];
    let body =
        "let v=array<f32,7>(0.0,0.03125,0.125,0.5,2.0,4.0,16.0)[u32(p.x)];return vec4(v,v,v,0.25);";
    for samples in [1, 4] {
        let config = FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba16Float, samples);
        let (fixed, ids) = render(
            (&device, &queue),
            config,
            (ToneMap::Filmic, 1.0),
            &HdrPostSettings::default(),
            body,
        );
        assert_eq!(ids, [StageId::new("tone", 0)]);
        let (post, _) = render(
            (&device, &queue),
            config,
            (ToneMap::Filmic, 1.0),
            &HdrPostSettings {
                resolve: crate::PostResolve::fxaa(0.0),
                ..Default::default()
            },
            body,
        );
        assert_eq!(post, fixed, "disabled FXAA must retain exact fixed output");
        for (i, pixel) in fixed.chunks_exact(4).enumerate() {
            let expected = encoded(ToneMap::Filmic.evaluate(values[i % 7], 1.0));
            assert!((expected - (expected.floor() + 0.5)).abs() > 0.1);
            for &channel in &pixel[..3] {
                assert!((f64::from(channel) - expected.round()).abs() < f64::EPSILON);
            }
            assert_eq!(pixel[3], 64);
        }
    }
}

#[test]
fn filmic_bloom_precedes_exposure_and_curve() {
    let (device, queue) = crate::gpu_tests::gpu();
    let settings = HdrPostSettings {
        bloom: Some(Bloom {
            policies: BloomPolicies::default(),
            threshold: 0.0,
            knee: 0.0,
            intensity: 0.5,
            levels: 3,
        }),
        ..Default::default()
    };
    let config = FrameConfig::new([9, 7], wgpu::TextureFormat::Rgba16Float, 1);
    let (actual, _) = render(
        (&device, &queue),
        config,
        (ToneMap::Filmic, 0.5),
        &settings,
        "return vec4(0.5,0.5,0.5,0.75);",
    );
    let expected = encoded(ToneMap::Filmic.evaluate(0.75, 0.5));
    for pixel in actual.chunks_exact(4) {
        for &channel in &pixel[..3] {
            assert!((f64::from(channel) - expected).abs() <= 1.0);
        }
        assert_eq!(pixel[3], 191);
    }
}

#[test]
fn filmic_gpu_special_values_and_zero_exposure() {
    let (device, queue) = crate::gpu_tests::gpu();
    let body = "return vec4(bitcast<f32>(0x7f800000u),bitcast<f32>(0xff800000u),bitcast<f32>(0x7fc00000u),0.5);";
    for exposure in [0.0, 1.0] {
        for settings in [
            HdrPostSettings::default(),
            HdrPostSettings {
                resolve: crate::PostResolve::fxaa(0.0),
                ..Default::default()
            },
        ] {
            let config = FrameConfig::new([3, 1], wgpu::TextureFormat::Rgba16Float, 1);
            let (actual, _) = render(
                (&device, &queue),
                config,
                (ToneMap::Filmic, exposure),
                &settings,
                body,
            );
            let red = if exposure > 0.0 { 255 } else { 0 };
            assert!(actual.chunks_exact(4).all(|p| p == [red, 0, 0, 128]));
        }
    }
}

#[test]
fn hdr_chain_propagates_fxaa_quality_and_keeps_stage_ids() {
    let (device, queue) = crate::gpu_tests::gpu();
    let config = FrameConfig::new([9, 7], wgpu::TextureFormat::Rgba16Float, 1);
    let body = "let v=select(0.125,4.0,p.x>p.y);return vec4(v,v*0.5,v*0.25,0.25);";
    let display = (ToneMap::Reinhard, 1.0);
    let (plain, plain_ids) = render(
        (&device, &queue),
        config,
        display,
        &HdrPostSettings::default(),
        body,
    );
    assert_eq!(plain_ids, [StageId::new("tone", 0)]);
    let settings = HdrPostSettings {
        resolve: crate::PostResolve::fxaa(0.75),
        ..Default::default()
    };
    let (filtered, ids) = render((&device, &queue), config, display, &settings, body);
    assert_eq!(ids, [StageId::new("tone", 0), StageId::new("fxaa", 0)]);
    assert_ne!(filtered, plain);
    for quality in [
        FxaaQuality {
            subpixel_blend: 0.0,
            ..Default::default()
        },
        FxaaQuality {
            minimum_threshold: 1.0,
            ..Default::default()
        },
        FxaaQuality {
            edge_threshold: 1.0,
            ..Default::default()
        },
    ] {
        let (actual, actual_ids) = render(
            (&device, &queue),
            config,
            display,
            &HdrPostSettings {
                resolve: crate::PostResolve::fxaa_with(0.75, quality),
                ..settings
            },
            body,
        );
        assert_eq!(actual, plain);
        assert_eq!(actual_ids, ids);
    }
}

#[test]
fn filmic_uses_published_adaptive_exposure() {
    let (device, queue) = crate::gpu_tests::gpu();
    let settings = HdrPostSettings {
        adaptive: Some(AdaptiveExposure {
            fixed_exposure: Some(0.5),
            ..super::adaptive()
        }),
        ..Default::default()
    };
    let config = FrameConfig::new([3, 3], wgpu::TextureFormat::Rgba16Float, 1);
    let body = "return vec4(1.0,1.0,1.0,0.25);";
    let (actual, _) = render(
        (&device, &queue),
        config,
        (ToneMap::Filmic, 1.0),
        &settings,
        body,
    );
    let expected = encoded(ToneMap::Filmic.evaluate(1.0, 0.5));
    assert!((expected - (expected.floor() + 0.5)).abs() > 0.1);
    for pixel in actual.chunks_exact(4) {
        for &channel in &pixel[..3] {
            assert!((f64::from(channel) - expected.round()).abs() < f64::EPSILON);
        }
        assert_eq!(pixel[3], 64);
    }
}

#[path = "aces_fitted_gpu_tests.rs"]
mod aces_fitted;
