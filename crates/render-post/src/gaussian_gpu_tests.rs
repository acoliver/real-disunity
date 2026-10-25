use super::post_effect_tests::{chain, pixels, step};
use super::*;
use crate::{Bloom, BloomKernel, GaussianLevel, HdrPostSettings};
#[path = "gaussian_reference.rs"]
mod reference;

pub(super) fn settings(radius: u32, count: u32) -> HdrPostSettings {
    HdrPostSettings {
        bloom: Some(Bloom {
            threshold: 0.0,
            knee: 0.0,
            intensity: 1.0,
            levels: count,
        }),
        bloom_kernel: BloomKernel::SeparablePyramid(
            [GaussianLevel {
                radius,
                tint: [1.0; 3],
                weight: 1.0 / f32::from(u16::try_from(count).unwrap()),
            }; 8],
        ),
        ..HdrPostSettings::default()
    }
}

fn compare_levels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    c: &PostChain<crate::ToneMapOutput>,
    expected: &[reference::Image],
) {
    for (texture, image) in c.configured[0].resources.textures.iter().zip(expected) {
        assert_eq!(
            [texture.width(), texture.height()],
            image.size.map(u32::from)
        );
        let actual = pixels(device, queue, texture);
        for (pixel, expected) in actual.chunks_exact(8).zip(&image.pixels) {
            for (channel, expected) in pixel[..6].chunks_exact(2).zip(expected) {
                let actual = crate::tone_map_gpu_tests::half(channel);
                // Half-float stores and hardware bilinear interpolation accumulate rounding.
                let tolerance = 0.004 * expected.abs() + 0.000_1;
                assert!(
                    (actual - expected).abs() <= tolerance,
                    "{actual} != {expected}, tolerance {tolerance}"
                );
            }
        }
    }
}

#[test]
fn gpu_gaussian_impulses_match_discrete_cpu_at_every_odd_level() {
    let (device, queue) = crate::gpu_tests::gpu();
    for radius in [0, 1, 4, 44, 64] {
        let policy = settings(radius, 6);
        let c = chain(&device, policy, [37, 23]);
        for [x, y] in [[18, 11], [36, 22]] {
            step(
                &device,
                &queue,
                &c,
                0,
                &format!(
                    "let v=select(0.0,16.0,u32(p.x)=={x}u && u32(p.y)=={y}u);return vec4<f32>(v,v*0.5,v*0.25,1.0);"
                ),
                false,
            );
            let input = reference::Image::new([37, 23], |px, py| {
                if [px, py] == [x, y] {
                    [16.0, 8.0, 4.0]
                } else {
                    [0.0; 3]
                }
            });
            let BloomKernel::SeparablePyramid(levels) = policy.bloom_kernel else {
                unreachable!()
            };
            let expected = reference::pyramid(&input, policy.bloom.unwrap(), &levels);
            compare_levels(&device, &queue, &c, &expected);
        }
    }
}

#[test]
fn gpu_gaussian_constant_knee_tint_and_weight_match_cpu() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut policy = settings(44, 6);
    policy.bloom.as_mut().unwrap().threshold = 1.0;
    policy.bloom.as_mut().unwrap().knee = 0.5;
    let BloomKernel::SeparablePyramid(ref mut levels) = policy.bloom_kernel else {
        unreachable!()
    };
    levels[2].tint = [0.5, 1.5, 2.0];
    let c = chain(&device, policy, [37, 23]);
    for value in [0.25, 0.75, 1.0, 3.25] {
        step(
            &device,
            &queue,
            &c,
            0,
            &format!("return vec4<f32>({value},{value},{value},1.0);"),
            false,
        );
        let input = reference::Image::new([37, 23], |_, _| [value; 3]);
        let BloomKernel::SeparablePyramid(levels) = policy.bloom_kernel else {
            unreachable!()
        };
        compare_levels(
            &device,
            &queue,
            &c,
            &reference::pyramid(&input, policy.bloom.unwrap(), &levels),
        );
    }
}

#[test]
fn gpu_gaussian_replacement_validates_before_retiring_targets() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut c = chain(&device, settings(44, 6), [37, 23]);
    let old = c.configured[0].resources.textures[0].clone();
    let before = c.targets().resolved().clone();
    assert!(c.resize(&device, [4, 4]).is_err());
    assert_eq!(&before, c.targets().resolved());
    assert_eq!(old, c.configured[0].resources.textures[0]);
    c.resize(&device, [39, 25]).unwrap();
    assert_ne!(old, c.configured[0].resources.textures[0]);
    let invalid = c
        .factory()
        .clone()
        .with_effects(&device, settings(44, 8))
        .unwrap();
    assert!(c.reconfigure(&device, invalid).is_err());
    let plain = c
        .factory()
        .clone()
        .with_effects(&device, HdrPostSettings::default())
        .unwrap();
    c.reconfigure(&device, plain).unwrap();
    assert!(c.configured[0].resources.textures.is_empty());
    assert_eq!(c.configured[0].stages.len(), 1);
    step(&device, &queue, &c, 0, "return vec4<f32>(1.0);", false);
}

#[test]
fn gpu_gaussian_cancelled_encoder_publishes_no_pixels_or_timings() {
    let (device, queue) =
        crate::gpu_tests::gpu_with_optional_features(wgpu::Features::TIMESTAMP_QUERY);
    let c = chain(&device, settings(4, 6), [37, 23]);
    c.configure_timing(&device, &queue, crate::TimingMode::Optional)
        .unwrap();
    let out = step(
        &device,
        &queue,
        &c,
        0,
        "return vec4<f32>(0.25,0.5,0.75,1.0);",
        false,
    );
    let expected = pixels(&device, &queue, &out);
    let authority = crate::test_authority(&queue);
    let mut encoder = authority.begin(&device).unwrap();
    c.begin_timing(42).unwrap();
    super::post_effect_tests::paint(
        &device,
        &mut encoder,
        c.targets().resolved(),
        "return vec4<f32>(16.0);",
    );
    c.encode(
        &mut encoder,
        &out.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    c.finish_timing(&mut encoder).unwrap();
    drop(encoder);
    assert!(c.take_timings().unwrap().is_empty());
    assert_eq!(pixels(&device, &queue, &out), expected);
}

#[test]
fn gpu_gaussian_normalized_levels_preserve_constant_energy() {
    let (device, queue) = crate::gpu_tests::gpu();
    let policy = settings(44, 6);
    let c = chain(&device, policy, [37, 23]);
    step(
        &device,
        &queue,
        &c,
        0,
        "return vec4<f32>(3.25,1.5,0.75,1.0);",
        false,
    );
    let raw = pixels(&device, &queue, &c.configured[0].resources.textures[0]);
    for pixel in raw.chunks_exact(8) {
        for (actual, expected) in pixel[..6].chunks_exact(2).zip([3.25, 1.5, 0.75]) {
            assert!((crate::tone_map_gpu_tests::half(actual) - expected).abs() < 0.004 * expected);
        }
    }
}

#[test]
fn gpu_gaussian_final_intensity_resampling_and_alpha_match_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    let input = reference::Image::new([37, 23], |x, y| {
        if x == 18 && y == 11 {
            [16.0, 8.0, 4.0]
        } else {
            [0.0; 3]
        }
    });
    for intensity in [0.0, 0.5, 1.0] {
        let mut policy = settings(4, 6);
        policy.bloom.as_mut().unwrap().intensity = intensity;
        let c = chain(&device, policy, [37, 23]);
        let out = step(
            &device,
            &queue,
            &c,
            0,
            "let v=select(0.0,16.0,u32(p.x)==18u && u32(p.y)==11u);return vec4<f32>(v,v*0.5,v*0.25,0.25);",
            false,
        );
        let BloomKernel::SeparablePyramid(levels) = policy.bloom_kernel else {
            unreachable!()
        };
        let bloom = reference::pyramid(&input, policy.bloom.unwrap(), &levels);
        let actual = pixels(&device, &queue, &out);
        for (index, pixel) in actual.chunks_exact(4).enumerate() {
            let x = i32::try_from(index % 37).unwrap();
            let y = i32::try_from(index / 37).unwrap();
            let base = input.load(x, y);
            let bloom = bloom[0].scaled(x, y, input.size);
            for channel in 0..3 {
                let linear = crate::ToneMapOutput::evaluate(
                    base[channel] + bloom[channel] * f64::from(intensity),
                    1.0,
                );
                let srgb = if linear <= 0.003_130_8 {
                    12.92 * linear
                } else {
                    1.055 * linear.powf(1.0 / 2.4) - 0.055
                };
                // Half storage, interpolation and attachment conversion allow 1.5 encoded steps.
                assert!((f64::from(pixel[channel]) - srgb * 255.0).abs() <= 1.5);
            }
            assert!((f64::from(pixel[3]) - 0.25 * 255.0).abs() <= 0.5);
        }
    }
}

#[test]
fn gpu_gaussian_kernel_without_bloom_schedules_only_output() {
    let (device, _) = crate::gpu_tests::gpu();
    let disabled = HdrPostSettings {
        bloom: None,
        ..settings(44, 6)
    };
    let c = chain(&device, disabled, [37, 23]);
    assert_eq!(c.configured[0].stages.len(), 1);
    assert!(c.configured[0].resources.textures.is_empty());
}

#[test]
fn gpu_disabled_gaussian_kernel_preserves_other_effects_output() {
    let (device, queue) = crate::gpu_tests::gpu();
    let render = |bloom_kernel| {
        let policy = HdrPostSettings {
            bloom: None,
            bloom_kernel,
            fxaa: Some(0.0),
            ..HdrPostSettings::default()
        };
        let c = chain(&device, policy, [9, 7]);
        let out = step(
            &device,
            &queue,
            &c,
            0,
            "let v=select(1.0,bitcast<f32>(0x7fc00000u),u32(p.x)==4u && u32(p.y)==3u);return vec4<f32>(v,v,v,1.0);",
            false,
        );
        pixels(&device, &queue, &out)
    };
    assert_eq!(
        render(BloomKernel::Tent),
        render(settings(44, 6).bloom_kernel)
    );
}
