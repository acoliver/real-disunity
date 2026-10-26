use super::*;
use crate::ssao::tests::{context, draw, read, upload};

fn assert_strength_reference(micro: u32, index: usize, pixel: &[f32]) {
    let strength = f64::from(micro) / 1_000_000.0;
    let squared = if index == 0 { 0.0001 } else { 0.0901 };
    let contribution = (1.0 - squared) * 0.01 / (0.0002 + squared);
    let blend = (1.1 - strength * 0.15).clamp(0.0, 1.0);
    let expected = contribution * (contribution * (1.0 - blend) + (0.9 + 0.5 * strength) * blend);
    assert!(
        pixel[0].is_finite() && pixel[0] >= 0.0,
        "{micro}: {pixel:?}"
    );
    assert!((f64::from(pixel[0]) - expected).abs() < expected * 2e-6 + 1e-6);
    let visibility = (1.0 - (expected * (strength * 1.5).min(1.0)).sqrt()).max(0.0);
    assert!(pixel[1].is_finite() && (0.0..=1.0).contains(&pixel[1]));
    assert!((f64::from(pixel[1]) - visibility).abs() < 2e-6);
    if micro == 0 {
        assert_eq!(pixel[1].to_bits(), 1.0f32.to_bits());
    } else if micro == 1 {
        assert!(pixel[1] > 0.99 && pixel[1] < 1.0);
    }
    if micro >= 666_667 {
        let original_blend = 1.1 - strength * 0.15;
        let original = contribution
            * (contribution * (1.0 - original_blend) + (0.9 + 0.5 * strength) * original_blend);
        assert!((f64::from(pixel[0]) - original).abs() < original * 2e-6 + 1e-6);
    }
}

#[test]
fn gpu_scalable_filtered_depth_selects_every_level_without_texel_snapping() {
    let context = context();
    let source = scalable::source(8);
    let prefix = source.split("@fragment").next().unwrap();
    let shader = format!(
        "{prefix}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ let level = i32(p.x); let value = depth_at(vec2<f32>(0.375, 0.5), level); return vec4<f32>(value, depth_at(vec2<f32>(-1.0), level), depth_at(vec2<f32>(2.0), level), textureLoad(surface_normals, vec2<i32>(0), 0).r); }}"
    );
    let pipeline = pipeline_format(context.device(), &shader, wgpu::TextureFormat::Rgba32Float);
    let normal = upload(&context, [1, 1], &[1.0]);
    let levels: Vec<_> = (0..8u16)
        .map(|index| {
            let base = f32::from(index) * 10.0;
            upload(
                &context,
                [2, 2],
                &[base, base + 2.0, base + 4.0, base + 6.0],
            )
        })
        .collect();
    let mut inputs = vec![&normal];
    inputs.extend(levels.iter());
    let group = group(context.device(), &pipeline, None, &inputs);
    let target = texture(context.device(), [8, 1], wgpu::TextureFormat::Rgba32Float);
    draw(&context, &pipeline, &group, &target);
    for (index, pixel) in read(&context, &target).chunks_exact(4).enumerate() {
        let base = f32::from(u16::try_from(index).unwrap()) * 10.0;
        assert_eq!(pixel, [base + 2.5, base, base + 6.0, 1.0]);
    }
}

#[test]
fn scalable_response_exponent_and_intensity_are_independent() {
    let context = context();
    let source = scalable::source(1);
    let prefix = source.split("@fragment").next().unwrap();
    let shader = format!(
        "{prefix}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) f32 {{ return visibility(p.x - 0.5, 4u); }}"
    );
    let pipeline = pipeline(context.device(), &shader);
    let output = texture(context.device(), [5, 1], wgpu::TextureFormat::R32Float);
    for (intensity, power) in [(1.0f32, 2.0f32), (1.0, 0.5), (0.0, 2.0)] {
        let mut values = vec![0.0; 56];
        values[49] = intensity;
        values[54] = power;
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("response parameters"),
            size: 224,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        context.queue().write_buffer(
            &uniform,
            0,
            &values
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let group = group(context.device(), &pipeline, Some(&uniform), &[]);
        draw(&context, &pipeline, &group, &output);
        for (index, actual) in read(&context, &output).iter().enumerate() {
            let mean = f32::from(u16::try_from(index).unwrap()) / 4.0;
            let expected = if intensity == 0.0 {
                1.0
            } else {
                (1.0 - mean.sqrt()).max(0.0).powf(power)
            };
            assert!((actual - expected).abs() < 1e-6);
        }
    }
}

#[test]
fn gpu_scalable_zero_bias_strength_matches_production_reference() {
    let context = context();
    let source = scalable::source(1);
    let prefix = source.split("@fragment").next().unwrap();
    let shader = format!(
        "{prefix}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ let delta = vec3<f32>(select(0.0, 0.3, p.x > 1.0), 0.0, 0.01); let sum = obscurance(delta, vec3<f32>(0.0, 0.0, 1.0)); return vec4<f32>(sum, visibility(sum, 1u), 0.0, 0.0); }}"
    );
    let pipeline = pipeline_format(context.device(), &shader, wgpu::TextureFormat::Rgba32Float);
    let output = texture(context.device(), [2, 1], wgpu::TextureFormat::Rgba32Float);
    let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("strength reference parameters"),
        size: 224,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = group(context.device(), &pipeline, Some(&uniform), &[]);
    for micro in [
        0u32, 1, 100_000, 500_000, 666_666, 666_667, 1_000_000, 2_000_000,
    ] {
        let mut values = vec![0.0f32; 56];
        values[48] = 1.0;
        values[49] = super::super::scalar(u64::from(micro)) / 1_000_000.0;
        values[54] = 1.0;
        context.queue().write_buffer(
            &uniform,
            0,
            &values
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        draw(&context, &pipeline, &group, &output);
        for (index, pixel) in read(&context, &output).chunks_exact(4).enumerate() {
            assert_strength_reference(micro, index, pixel);
        }
    }
}

#[test]
fn gpu_scalable_projected_tap_radius_respects_camera_projection() {
    let context = context();
    let source = scalable::source(1);
    let prefix = source.split("@fragment").next().unwrap();
    let shader = format!(
        "{prefix}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ let depth = select(10.0, 20.0, p.x > 1.0); let radius = projected_radius(vec3<f32>(0.0, 0.0, -depth), 100u); return vec4<f32>(radius, length(tap_offset(9u, 10u, 0.0, radius)), 0.0, 0.0); }}"
    );
    let pipeline = pipeline_format(context.device(), &shader, wgpu::TextureFormat::Rgba32Float);
    let output = texture(context.device(), [2, 1], wgpu::TextureFormat::Rgba32Float);
    for orthographic in [false, true] {
        let projection = if orthographic {
            glam::camera::rh::proj::directx::orthographic(-2.0, 2.0, -2.0, 2.0, 0.1, 100.0)
        } else {
            glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0)
        };
        let mut values = vec![0.0f32; 56];
        values[16..32].copy_from_slice(&projection.to_cols_array());
        values[48] = 1.0;
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("projection radius parameters"),
            size: 224,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        context.queue().write_buffer(
            &uniform,
            0,
            &values
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let group = group(context.device(), &pipeline, Some(&uniform), &[]);
        draw(&context, &pipeline, &group, &output);
        for (index, pixel) in read(&context, &output).chunks_exact(4).enumerate() {
            let denominator = if orthographic {
                1.0
            } else if index == 0 {
                10.0
            } else {
                20.0
            };
            let expected = projection.y_axis.y * 50.0 / denominator;
            assert!((pixel[0] - expected).abs() < 1e-5);
            assert!((pixel[1] - expected * 0.95f32.powi(2)).abs() < 1e-5);
        }
    }
}
