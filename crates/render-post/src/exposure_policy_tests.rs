#[test]
fn gpu_output_scale_precedes_bloom_addition() {
    let (device, queue) = crate::gpu_tests::gpu();
    let settings = HdrPostSettings {
        adaptive: Some(policy(ExposureAdaptation::Instantaneous)),
        bloom: Some(Bloom {
            policies: BloomPolicies::default(),
            threshold: 0.0,
            knee: 0.0,
            intensity: 0.5,
            levels: 1,
        }),
        ..HdrPostSettings::default()
    };
    let c = PostChain::new(
        &device,
        FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
            .unwrap()
            .with_effects(&device, settings)
            .unwrap(),
    )
    .unwrap();
    let out = color_texture(
        &device,
        "metering composition output",
        [8, 8],
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    super::paint(
        &device,
        &mut encoder,
        c.targets().resolved(),
        "return vec4<f32>(0.25,0.25,0.25,1.0);",
    );
    c.factory()
        .prepare_effects(&mut encoder, 0, 0.25, false)
        .unwrap();
    c.encode(
        &mut encoder,
        &out.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Poll).unwrap();
    let bytes = Readback::new(&device, FrameConfig::new([8, 8], out.format(), 1), 100_000)
        .unwrap()
        .read(&device, &crate::test_authority(&queue), &out)
        .unwrap();
    let observed = bytes[0];
    let raw = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
    let exposure = f32::from_le_bytes(raw[..4].try_into().unwrap());
    let linear = 0.25 * f64::from(exposure) * 4.0 + 0.125;
    let code = |l: f64| crate::fxaa_tolerance_tests::encoded_steps(l / (1.0 + l), true);
    // The bloom term enters the sum after it passed at most three `Rgba16Float` stores (prefilter,
    // one reduction, one reconstruction). Each errs by at most `2^-10` of the stored 0.25 (the
    // module comment in `fxaa_tolerance_tests` covers rounding toward zero), scaled by the
    // intensity 0.5. The display code is monotone in that term, so its effect is the code
    // difference at `linear` shifted by the term, taking the larger side. The remaining arithmetic (a product, a sum, a
    // division) is inside the shared 8-bit output bound.
    let bloom_error = 3.0 * 0.25 / 1024.0 * 0.5;
    let bloom_steps =
        (code(linear + bloom_error) - code(linear)).max(code(linear) - code(linear - bloom_error));
    crate::fxaa_tolerance_tests::assert_code_within(
        observed,
        code(linear),
        bloom_steps,
        "output scale precedes bloom",
    );
}

fn submit_sample(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    c: &PostChain<ToneMapOutput>,
    value: f32,
    step: (usize, f32, bool),
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    super::paint(
        device,
        &mut encoder,
        c.targets().resolved(),
        &format!("return vec4<f32>({value},{value},{value},1.0);"),
    );
    c.factory()
        .prepare_effects(&mut encoder, step.0, step.1, step.2)
        .unwrap();
    let output = super::output(device, c.targets().config().size);
    c.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Poll).unwrap();
}

fn measured(value: f32, a: AdaptiveExposure) -> f64 {
    let mut bins = [0; 256];
    bins[metering_bin(value, a)] = 64;
    percentile_luminance(&bins, a.log_range, 0.575)
}

#[test]
fn gpu_luminance_history_pause_reset_and_unsubmitted_cancellation() {
    let (device, queue) = crate::gpu_tests::gpu();
    let a = AdaptiveExposure {
        fixed_delta: None,
        ..policy(ExposureAdaptation::Luminance)
    };
    let mut c = chain(
        &device,
        &HdrPostSettings {
            adaptive: Some(a),
            ..Default::default()
        },
        [8, 8],
    );
    submit_sample(&device, &queue, &c, 0.125, (0, 0.25, false));
    let first = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
    c.select_frame_slot(1);
    submit_sample(&device, &queue, &c, 2.0, (1, 0.25, false));
    let second = buffer(&device, &queue, c.factory().exposure_buffer(1).unwrap());
    let l = f32::from_le_bytes(second[8..12].try_into().unwrap());
    let previous = measured(0.125, a);
    let expected = previous + (measured(2.0, a) - previous) * (1.0 - (-0.75_f64).exp());
    assert!((f64::from(l) - expected).abs() < 1e-6);
    assert_ne!(first, second);
    c.select_frame_slot(2);
    submit_sample(&device, &queue, &c, 0.125, (2, 0.0, false));
    assert_eq!(
        buffer(&device, &queue, c.factory().exposure_buffer(2).unwrap()),
        second
    );
    let mut cancelled = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    c.factory()
        .prepare_effects(&mut cancelled, 2, 1.0, true)
        .unwrap();
    drop(cancelled);
    submit_sample(&device, &queue, &c, 0.125, (2, 0.0, false));
    assert_eq!(
        buffer(&device, &queue, c.factory().exposure_buffer(2).unwrap()),
        second
    );
    submit_sample(&device, &queue, &c, 0.125, (2, 0.25, true));
    assert_eq!(
        buffer(&device, &queue, c.factory().exposure_buffer(2).unwrap()),
        first
    );
}

use super::{adaptive, buffer, chain, step};
use crate::*;

fn policy(adaptation: ExposureAdaptation) -> AdaptiveExposure {
    AdaptiveExposure {
        metering: ExposureMetering::InterpolatedPercentile(PercentileExposure {
            black_threshold: 0.005,
            percentile: 0.575,
            key_bias: 0.575_608_2,
            output_scale: 4.0,
            adaptation,
        }),
        log_range: [-10.0, 2.0],
        exposure_range: [0.000_01, 64.0],
        ..adaptive()
    }
}
#[test]
fn bins_counts_boundaries_interpolation_and_black_policy() {
    let a = policy(ExposureAdaptation::Instantaneous);
    let mut bins = [0; 256];
    for (v, expected) in [
        (0.0, 0),
        (0.004, 0),
        (0.005, 51),
        (1.0, 213),
        (4.0, 255),
        (16.0, 255),
    ] {
        assert_eq!(metering_bin(v, a), expected);
        bins[expected] += 1;
    }
    assert_eq!(bins.iter().sum::<u32>(), 6);
    assert_eq!(bins[0], 2);
    let mut bins = [0; 256];
    bins[20] = 2;
    bins[100] = 2;
    let expected = ((100.0_f64) / 256.0 * 12.0 - 10.0).exp2();
    assert!((percentile_luminance(&bins, a.log_range, 0.5) - expected).abs() < 1e-8);
    let expected = ((20.5_f64 - 1.0) / 256.0 * 12.0 - 10.0).exp2();
    assert!((percentile_luminance(&bins, a.log_range, 0.25) - expected).abs() < 1e-8);
    bins = [0; 256];
    let empty = percentile_luminance(&bins, a.log_range, 0.575);
    assert!((empty - (-10.046_875_f64).exp2()).abs() < 1e-9);
    bins[0] = 1000;
    let black = percentile_luminance(&bins, a.log_range, 0.575);
    assert!((black - ((0.425_f64 - 1.0) / 256.0 * 12.0 - 10.0).exp2()).abs() < 1e-9);
}
#[test]
fn key_references_floor_and_invalid_settings() {
    for (l, expected) in [(0.0, 0.03), (9.0, 1.03 - 2.0 / 3.0), (99.0, 0.53)] {
        assert!((luminance_key(l, 1.0) - expected).abs() < 1e-6);
        assert!((luminance_key(l, 0.5) - expected * 0.5).abs() < 1e-6);
    }
    let mut a = policy(ExposureAdaptation::Instantaneous);
    a.log_range = [20.0, 24.0];
    let mut bins = [0; 256];
    bins[255] = 10;
    assert!((target_exposure(&bins, a) - 0.000_01).abs() < 1e-10);
    let ExposureMetering::InterpolatedPercentile(p) = a.metering else {
        unreachable!()
    };
    for invalid in [
        PercentileExposure {
            percentile: 1.0,
            ..p
        },
        PercentileExposure {
            percentile: f32::NAN,
            ..p
        },
        PercentileExposure {
            black_threshold: 0.0,
            ..p
        },
        PercentileExposure {
            key_bias: f32::INFINITY,
            ..p
        },
        PercentileExposure {
            output_scale: 0.0,
            ..p
        },
    ] {
        a.metering = ExposureMetering::InterpolatedPercentile(invalid);
        assert!(
            HdrPostSettings {
                adaptive: Some(a),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
#[test]
fn gpu_percentile_matches_cpu_histogram_exposure_and_adaptation() {
    let (device, queue) = crate::gpu_tests::gpu();
    for mode in [
        ExposureAdaptation::Exposure,
        ExposureAdaptation::Instantaneous,
        ExposureAdaptation::Luminance,
    ] {
        let a = policy(mode);
        let c = chain(
            &device,
            &HdrPostSettings {
                adaptive: Some(a),
                ..Default::default()
            },
            [16, 8],
        );
        step(
            &device,
            &queue,
            &c,
            0,
            "let v=array<f32,4>(0.0,0.1875,0.75,3.25)[u32(p.x)%4u]; return vec4<f32>(v,v,v,1.0);",
            false,
        );
        let raw = buffer(&device, &queue, &c.configured[0].resources.buffers[0]);
        let actual: Vec<_> = raw
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let mut bins = [0; 256];
        for v in [0.0, 0.1875, 0.75, 3.25] {
            bins[metering_bin(v, a)] += 32;
        }
        assert_eq!(actual, bins);
        let target = target_exposure(&bins, a);
        let expected = if mode == ExposureAdaptation::Exposure {
            1.0 + (target - 1.0)
                * (1.0 - (-f64::from(a.speeds[usize::from(target < 1.0)]) * 0.25).exp())
        } else {
            target
        };
        let raw = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
        let observed = f32::from_le_bytes(raw[..4].try_into().unwrap());
        assert!(
            (f64::from(observed) - expected).abs() < 1e-6,
            "{mode:?}: {observed} != {expected}"
        );
    }
}
