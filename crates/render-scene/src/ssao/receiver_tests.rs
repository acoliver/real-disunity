fn scalable_settings() -> worldrunner_render_plan::SceneSsao {
    worldrunner_render_plan::SceneSsao {
        kernel: worldrunner_render_plan::SceneSsaoKernel::Scalable {
            turns_micro: 7_000_000,
            power_micro: 1_000_000,
        },
        filter: worldrunner_render_plan::SceneSsaoFilter::ScalableBilateral {
            radius: 4,
            step: 2,
            sigma_scale_micro: 1_500_000,
            sharpness_micro: 7_000_000,
        },
        ..super::tests::settings()
    }
}

#[test]
fn gpu_final_filter_preserves_consumed_metre_depth_for_each_policy() {
    use worldrunner_render_plan::{SceneSsaoFilter, SceneSsaoKernel};
    let context = context();
    for settings in [
        super::tests::settings(),
        worldrunner_render_plan::SceneSsao {
            kernel: SceneSsaoKernel::Spiral,
            filter: SceneSsaoFilter::SeparableBilateral,
            ..super::tests::settings()
        },
        worldrunner_render_plan::SceneSsao {
            kernel: SceneSsaoKernel::Spiral,
            ..super::tests::settings()
        },
        scalable_settings(),
    ] {
        let mut settings = settings;
        settings.receiver = SceneSsaoReceiver::DepthAwareGather;
        let metadata =
            crate::scene_metadata::SceneMetadata::new(context.device(), [5, 3], settings).unwrap();
        let depth = metadata.passes.receiver_depth(settings.kernel);
        let size = [depth.width(), depth.height()];
        let count = usize::try_from(size[0] * size[1]).unwrap();
        let values: Vec<_> = (0..count)
            .map(|i| 100.013 + f32::from(u16::try_from(i).unwrap()) * 0.271)
            .collect();
        let values = prepare_depth(&context, &metadata, settings.kernel, &values);
        metadata.passes.draw_final_filter(&context);
        let actual = read(&context, &metadata.passes.filtered);
        for (pair, expected) in actual.chunks_exact(2).zip(values) {
            assert!(
                (pair[1] - expected).abs() < 2e-5,
                "{} != {expected}",
                pair[1]
            );
        }
    }
}

fn prepare_depth(
    context: &crate::device::GpuContext,
    metadata: &crate::scene_metadata::SceneMetadata,
    kernel: worldrunner_render_plan::SceneSsaoKernel,
    values: &[f32],
) -> Vec<f32> {
    let input = metadata.passes.receiver_depth(kernel);
    let size = [input.width(), input.height()];
    let spiral = kernel == worldrunner_render_plan::SceneSsaoKernel::Spiral;
    let destination = if spiral {
        metadata.passes.depth_levels()[0]
    } else {
        input
    };
    let uploaded = upload(context, size, values);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encoder.copy_texture_to_texture(
        uploaded.as_image_copy(),
        destination.as_image_copy(),
        destination.size(),
    );
    encoder.finish().submit().unwrap();
    if !spiral {
        return values.to_vec();
    }
    let normals: Vec<_> = (0..values.len())
        .flat_map(|i| [0.0_f32, 0.0, 1.0, if i % 3 == 0 { 0.0 } else { 1.0 }])
        .flat_map(|v| half::f16::from_f32(v).to_le_bytes())
        .collect();
    context.queue().write_texture(
        metadata.normals.as_image_copy(),
        &normals,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 8),
            rows_per_image: Some(size[1]),
        },
        metadata.normals.size(),
    );
    metadata.passes.draw_receiver_metadata(context);
    let bytes = worldrunner_render_post::Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(size, input.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), input)
    .unwrap();
    let actual = crate::offscreen::ScalarImage::from_native(
        crate::Viewport {
            width: size[0],
            height: size[1],
        },
        input.format(),
        &bytes,
    )
    .unwrap()
    .pixels;
    assert_eq!(actual[0].to_bits(), 0.0_f32.to_bits());
    assert_ne!(actual[1].to_bits(), values[1].to_bits());
    actual
}

#[test]
fn receiver_choices_validate_and_account_for_only_final_target_growth() {
    use crate::compositor::RendererConfiguration;
    let mut config = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    config.ssao = Some(super::tests::settings());
    let limits = wgpu::Limits::default();
    for mut settings in [super::tests::settings(), scalable_settings()] {
        settings.receiver = SceneSsaoReceiver::Nearest;
        assert!(settings.validate().is_ok());
        config.ssao = Some(settings);
        let before = config.allocations().validate(&limits).unwrap();
        settings.receiver = SceneSsaoReceiver::DepthAwareGather;
        assert!(settings.validate().is_ok());
        config.ssao = Some(settings);
        let after = config.allocations().validate(&limits).unwrap();
        let size = if matches!(
            settings.kernel,
            worldrunner_render_plan::SceneSsaoKernel::Scalable { .. }
        ) {
            settings.depth_footprint.rounding.half_extent([5, 3])
        } else {
            [5, 3]
        };
        assert_eq!(after - before, u64::from(size[0] * size[1]) * 4);
    }
}

use super::tests::{context, draw, read, upload};
use worldrunner_render_plan::SceneSsaoReceiver;

fn oracle(ao: [f64; 4], depths: [f64; 4], receiver: f64, self_ao: f64) -> f64 {
    let covered: Vec<_> = depths.into_iter().filter(|depth| *depth > 0.0).collect();
    if covered.is_empty() {
        return 1.01_f64.min(self_ao);
    }
    let minimum = covered.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = covered.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let depth = receiver.clamp(minimum, maximum);
    let weights = depths.map(|tap| {
        if tap > 0.0 {
            (-(depth - tap).abs()).exp()
        } else {
            0.0
        }
    });
    let sum: f64 = ao.into_iter().zip(weights).map(|(a, w)| a * w).sum();
    (1.01 * sum / weights.into_iter().sum::<f64>()).min(self_ao)
}

#[test]
fn receiver_oracle_constants_clamp_weights_composition_and_depth_shift() {
    assert!((oracle([0.5; 4], [2.0, 3.0, 4.0, 5.0], 3.0, 1.0) - 0.505).abs() < 1e-12);
    let ao = [0.1, 0.2, 0.8, 0.9];
    let depths = [2.0, 2.0, 4.0, 4.0];
    let weight = (-2.0_f64).exp();
    let expected = 1.01 * (0.3 + 1.7 * weight) / (2.0 + 2.0 * weight);
    assert!((oracle(ao, depths, -20.0, 1.0) - expected).abs() < 1e-12);
    assert!((oracle(ao, depths, 40.0, 0.6) - 0.6).abs() < 1e-12);
    assert!(
        (oracle(ao, depths, 3.0, 1.0) - oracle(ao, depths.map(|d| d + 100.0), 103.0, 1.0)).abs()
            < 1e-12
    );
}

#[test]
fn receiver_oracle_excludes_uncovered_taps() {
    for depth in [1.0, 101.0] {
        assert!(
            (oracle([0.2, 0.2, 1.0, 1.0], [depth, depth, 0.0, 0.0], depth, 1.0) - 0.202).abs()
                < 1e-12
        );
    }
    assert_eq!(
        oracle([0.2; 4], [0.0, -1.0, 0.0, -2.0], 1.0, 1.0).to_bits(),
        1.0_f64.to_bits()
    );
}

#[test]
fn gpu_receiver_gather_excludes_uncovered_silhouette_taps() {
    let context = context();
    for (depths, receiver, expected) in [
        ([1.0_f32, 1.0, 0.0, 0.0], 1.0, 0.2),
        ([101.0, 101.0, 0.0, 0.0], 101.0, 0.2),
        ([0.0; 4], 1.0, 1.0),
        ([0.0, -1.0, 0.0, -2.0], 101.0, 1.0),
    ] {
        let packed: Vec<_> = [0.2_f32, 0.2, 1.0, 1.0]
            .into_iter()
            .zip(depths)
            .flat_map(|(ao, depth)| [ao, depth])
            .flat_map(f32::to_le_bytes)
            .collect();
        let texture =
            super::passes::texture(context.device(), [2, 2], wgpu::TextureFormat::Rg32Float);
        context.queue().write_texture(
            texture.as_image_copy(),
            &packed,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(2),
            },
            texture.size(),
        );
        let shader = format!(
            "@group(0) @binding(1) var ambient_texture: texture_2d<f32>;\n{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) f32 {{ let ao = receiver_gather(vec2(0.5), {receiver}); return select(ao, min(1.01 * ao, 1.0), p.x > 1.0); }}",
            include_str!("receiver.wgsl")
        );
        let pipeline = super::passes::pipeline(context.device(), &shader);
        let group = super::passes::group(context.device(), &pipeline, None, &[&texture]);
        let output = upload(&context, [2, 1], &[0.0; 2]);
        draw(&context, &pipeline, &group, &output);
        let actual = read(&context, &output);
        assert!(
            (actual[0] - expected).abs() < 2e-6,
            "raw AO: {actual:?}, depth: {receiver}"
        );
        assert!(
            (actual[1] - (1.01 * expected).min(1.0)).abs() < 2e-6,
            "composed AO: {actual:?}, depth: {receiver}"
        );
    }
}

#[test]
fn nearest_receiver_preserves_shader_bytes_and_gather_is_shared() {
    assert_eq!(SceneSsaoReceiver::default(), SceneSsaoReceiver::Nearest);
    for source in [crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL] {
        let nearest = super::ambient_source(source);
        assert_eq!(
            super::receiver::source(&nearest, SceneSsaoReceiver::Nearest),
            nearest
        );
        let gather = super::receiver::source(&nearest, SceneSsaoReceiver::DepthAwareGather);
        assert!(gather.contains("min(1.01 * screen, material_ao)"));
        assert!(gather.contains(include_str!("receiver.wgsl")));
    }
}

#[test]
fn gpu_receiver_gather_matches_oracle_and_depth_shift() {
    let context = context();
    let ao = [0.125_f32, 0.25, 0.75, 0.875];
    for shift in [0.0_f32, 100.0] {
        let depths = [2.0 + shift, 2.0 + shift, 4.0 + shift, 4.0 + shift];
        let packed: Vec<_> = ao
            .into_iter()
            .zip(depths)
            .flat_map(|(a, d)| [a, d])
            .collect();
        let texture =
            super::passes::texture(context.device(), [2, 2], wgpu::TextureFormat::Rg32Float);
        context.queue().write_texture(
            texture.as_image_copy(),
            &packed
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(2),
            },
            texture.size(),
        );
        let shader = format!(
            "@group(0) @binding(1) var ambient_texture: texture_2d<f32>;\n{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) f32 {{ return min(1.01 * receiver_gather(vec2(0.5), p.x - 2.5 + {shift}), 0.7); }}",
            include_str!("receiver.wgsl")
        );
        let pipeline = super::passes::pipeline(context.device(), &shader);
        let group = super::passes::group(context.device(), &pipeline, None, &[&texture]);
        let output = upload(&context, [8, 1], &[0.0; 8]);
        draw(&context, &pipeline, &group, &output);
        for (index, value) in read(&context, &output).into_iter().enumerate() {
            let receiver = f64::from(u16::try_from(index).unwrap()) - 2.0 + f64::from(shift);
            let expected = oracle(ao.map(f64::from), depths.map(f64::from), receiver, 0.7);
            assert!(
                (f64::from(value) - expected).abs() < 2e-6,
                "{value} != {expected}"
            );
        }
    }
}
