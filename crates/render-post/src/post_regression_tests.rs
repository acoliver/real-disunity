use super::post_effect_tests::*;
use super::*;
use crate::{
    AdaptiveExposure, Bloom, BufferReadback, HdrPostSettings, histogram_bin, target_exposure,
};

#[test]
fn gpu_fixed_delta_overrides_invalid_host_timing_before_validation() {
    let (device, queue) = crate::gpu_tests::gpu();
    let c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(adaptive()),
            ..HdrPostSettings::default()
        },
        [8, 8],
    );
    let body = "return vec4<f32>(0.1875,0.1875,0.1875,1.0);";
    let expected = step(&device, &queue, &c, 0, body, true);
    let expected = pixels(&device, &queue, &expected);
    for delta in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 10.01] {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let out = output(&device, [8, 8]);
        paint(&device, &mut encoder, c.targets().resolved(), body);
        c.factory()
            .prepare_effects(&mut encoder, 0, delta, true)
            .unwrap();
        c.encode(
            &mut encoder,
            &out.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::Poll).unwrap();
        assert_eq!(pixels(&device, &queue, &out), expected);
    }
}

#[test]
fn gpu_odd_bloom_pyramid_preserves_constants() {
    let (device, queue) = crate::gpu_tests::gpu();
    let c = chain(
        &device,
        HdrPostSettings {
            bloom: Some(Bloom {
                threshold: 0.0,
                knee: 0.0,
                intensity: 1.0,
                levels: 4,
            }),
            ..HdrPostSettings::default()
        },
        [9, 7],
    );
    step(
        &device,
        &queue,
        &c,
        0,
        "return vec4<f32>(3.25,3.25,3.25,1.0);",
        false,
    );
    for texture in &c.configured[0].resources.textures {
        let raw = pixels(&device, &queue, texture);
        assert!(raw.chunks_exact(8).all(|pixel| {
            pixel[..6].chunks_exact(2).all(|channel| {
                crate::tone_map_gpu_tests::half(channel).to_bits() == 3.25_f64.to_bits()
            })
        }));
    }
}

#[test]
fn gpu_odd_bloom_pyramid_keeps_the_final_edge_impulse() {
    let (device, queue) = crate::gpu_tests::gpu();
    let c = chain(
        &device,
        HdrPostSettings {
            bloom: Some(Bloom {
                threshold: 0.0,
                knee: 0.0,
                intensity: 1.0,
                levels: 4,
            }),
            ..HdrPostSettings::default()
        },
        [9, 7],
    );
    step(
        &device,
        &queue,
        &c,
        0,
        "let v=select(0.0,16.0,u32(p.x)==8u && u32(p.y)==6u);return vec4<f32>(v,v,v,1.0);",
        false,
    );
    let extents: Vec<_> = c.configured[0]
        .resources
        .textures
        .iter()
        .map(|t| [t.width(), t.height()])
        .collect();
    assert_eq!(
        extents,
        [[9, 7], [5, 4], [3, 2], [2, 1], [3, 2], [5, 4], [9, 7]]
    );
    for texture in &c.configured[0].resources.textures {
        let raw = pixels(&device, &queue, texture);
        let values: Vec<_> = raw
            .chunks_exact(8)
            .map(|pixel| crate::tone_map_gpu_tests::half(&pixel[..2]))
            .collect();
        let corner = *values.last().unwrap();
        assert!(corner > 0.0);
        assert!(
            values
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0 && *v <= corner)
        );
    }
}

fn ordered_sequence(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slots: &[usize],
    fixed_delta: Option<f32>,
) -> Vec<Vec<u8>> {
    let a = AdaptiveExposure {
        fixed_delta,
        ..adaptive()
    };
    let mut c = chain(
        device,
        HdrPostSettings {
            adaptive: Some(a),
            ..HdrPostSettings::default()
        },
        [8, 8],
    );
    let mut previous = 1.0;
    let mut results = Vec::new();
    for batch in 0..18 / slots.len() {
        if batch * slots.len() == 12 {
            c.resize(device, [9, 7]).unwrap();
        }
        let mut commands = Vec::new();
        let mut captures = Vec::new();
        for index in 0..slots.len() {
            let frame = batch * slots.len() + index;
            let slot = slots[(index + batch) % slots.len()];
            let radiance = [0.03125, 3.25, 0.75, 0.1875][frame % 4];
            let delta = [0.03125, 0.125, 0.375][frame % 3];
            let reset = frame == 6;
            if reset {
                previous = 1.0;
            }
            let mut bins = [0; 256];
            bins[histogram_bin(radiance, a.log_range)] = 1;
            let target = target_exposure(&bins, a);
            let speed = f64::from(a.speeds[usize::from(target < previous)]);
            previous += (target - previous)
                * (1.0 - (-speed * f64::from(fixed_delta.unwrap_or(delta))).exp());
            let (command, capture, out) =
                capture_step(device, &mut c, slot, radiance, delta, reset);
            commands.push(command);
            captures.push((capture, previous, out));
        }
        let submission = queue.submit(commands);
        device.poll(wgpu::PollType::Poll).unwrap();
        for (mut capture, expected, out) in captures {
            let raw = capture
                .read_encoded(device, Some(submission.clone()))
                .unwrap();
            let actual = f32::from_le_bytes(raw[..4].try_into().unwrap());
            assert!(
                (f64::from(actual) - expected).abs() < 0.000_01,
                "{actual} != {expected}"
            );
            results.push(pixels(device, queue, &out));
        }
    }
    results
}

#[test]
fn gpu_exposure_carries_in_submission_order_with_irregular_slot_reuse() {
    let (device, queue) = crate::gpu_tests::gpu();
    for fixed_delta in [None, Some(0.25)] {
        // Equal-length sequences with one, two and three admitted slots. Reading
        // each batch completes its submissions before any slot is reused.
        let single = ordered_sequence(&device, &queue, &[0], fixed_delta);
        let double = ordered_sequence(&device, &queue, &[1, 0], fixed_delta);
        let triple = ordered_sequence(&device, &queue, &[2, 0, 1], fixed_delta);
        assert_eq!(single, double);
        assert_eq!(single, triple);
    }
}

#[test]
fn gpu_empty_policy_removes_all_stages_and_restores_fixed_bytes() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(adaptive()),
            bloom: Some(Bloom {
                threshold: 0.0,
                knee: 0.5,
                intensity: 1.0,
                levels: 3,
            }),
            fxaa: Some(1.0),
            upscale: None,
            bloom_kernel: crate::BloomKernel::Tent,
        },
        [17, 13],
    );
    let body = "let v=select(0.1875,3.25,p.x>p.y);return vec4<f32>(v,v,v,1.0);";
    step(&device, &queue, &c, 0, body, false);
    let empty = c
        .factory()
        .clone()
        .with_effects(&device, HdrPostSettings::default())
        .unwrap();
    c.reconfigure(&device, empty).unwrap();
    assert!(c.factory().exposure_buffer(0).is_none());
    assert_eq!(c.configured.len(), 1);
    assert_eq!(c.configured[0].stages.len(), 1);
    assert_eq!(
        c.configured[0].stages[0].label,
        "fixed exposure and tone mapping"
    );
    let fresh = chain(&device, HdrPostSettings::default(), [17, 13]);
    for body in [body, "return vec4<f32>(0.75,0.75,0.75,1.0);"] {
        let actual = step(&device, &queue, &c, 0, body, false);
        let expected = step(&device, &queue, &fresh, 0, body, false);
        assert_eq!(
            pixels(&device, &queue, &actual),
            pixels(&device, &queue, &expected)
        );
    }
}

#[test]
fn gpu_invalid_effective_delta_is_typed_and_appends_no_upload() {
    let (device, queue) = crate::gpu_tests::gpu();
    let settings = HdrPostSettings {
        adaptive: Some(AdaptiveExposure {
            fixed_delta: None,
            ..adaptive()
        }),
        ..HdrPostSettings::default()
    };
    let c = chain(&device, settings, [8, 8]);
    let mut control = chain(&device, settings, [8, 8]);
    for delta in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 10.01] {
        let before = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        c.factory()
            .prepare_effects(&mut encoder, 0, 0.125, false)
            .unwrap();
        assert_eq!(
            c.factory().prepare_effects(&mut encoder, 0, delta, true),
            Err(PostError::FrameDelta)
        );
        assert_eq!(
            before,
            buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap())
        );
        let out = output(&device, [8, 8]);
        paint(
            &device,
            &mut encoder,
            c.targets().resolved(),
            "return vec4<f32>(0.1875,0.1875,0.1875,1.0);",
        );
        c.encode(
            &mut encoder,
            &out.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::Poll).unwrap();
        let actual_pixels = pixels(&device, &queue, &out);
        let (command, _, expected) = capture_step(&device, &mut control, 0, 0.1875, 0.125, false);
        queue.submit([command]);
        device.poll(wgpu::PollType::Poll).unwrap();
        assert_eq!(actual_pixels, pixels(&device, &queue, &expected));
        assert_eq!(
            buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap()),
            buffer(
                &device,
                &queue,
                control.factory().exposure_buffer(0).unwrap()
            )
        );
    }
}

fn capture_step(
    device: &wgpu::Device,
    c: &mut PostChain<crate::ToneMapOutput>,
    slot: usize,
    radiance: f32,
    delta: f32,
    reset: bool,
) -> (wgpu::CommandBuffer, BufferReadback, wgpu::Texture) {
    c.select_frame_slot(slot);
    let out = output(device, c.targets().config().size);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    paint(
        device,
        &mut encoder,
        c.targets().resolved(),
        &format!("return vec4<f32>({radiance},{radiance},{radiance},1.0);"),
    );
    c.factory()
        .prepare_effects(&mut encoder, slot, delta, reset)
        .unwrap();
    c.encode(
        &mut encoder,
        &out.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    let mut capture = BufferReadback::new(device, 16, 16).unwrap();
    capture
        .encode(&mut encoder, c.factory().exposure_buffer(slot).unwrap())
        .unwrap();
    (encoder.finish(), capture, out)
}

#[test]
fn gpu_delta_interval_includes_zero_and_ten() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(AdaptiveExposure {
                fixed_delta: None,
                ..adaptive()
            }),
            ..HdrPostSettings::default()
        },
        [8, 8],
    );
    for delta in [0.0, 10.0] {
        let (command, mut capture, _) = capture_step(&device, &mut c, 0, 0.1875, delta, false);
        let submission = queue.submit([command]);
        let raw = capture.read_encoded(&device, Some(submission)).unwrap();
        assert!(f32::from_le_bytes(raw[..4].try_into().unwrap()).is_finite());
    }
}
