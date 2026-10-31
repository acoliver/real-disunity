use super::*;
use crate::scene_metadata::SceneMetadata;
use crate::ssao::tests::{context, draw, read, settings, upload};
use worldrunner_render_plan::{SceneSsao, SceneSsaoFilter, SceneSsaoKernel};

fn corner_normals(context: &crate::device::GpuContext, normal: u16) -> wgpu::Texture {
    let normals = texture(context.device(), [66, 66], wgpu::TextureFormat::Rgba16Float);
    let bytes: Vec<_> = (0..4356)
        .flat_map(|_| [0, 0, normal, 0x3c00])
        .flat_map(u16::to_le_bytes)
        .collect();
    context.queue().write_texture(
        normals.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(66 * 8),
            rows_per_image: Some(66),
        },
        normals.size(),
    );
    normals
}

pub(super) fn recipe() -> SceneSsao {
    SceneSsao {
        kernel: SceneSsaoKernel::Scalable {
            turns_micro: 7_000_000,
            power_micro: 1_000_000,
        },
        filter: SceneSsaoFilter::ScalableBilateral {
            radius: 4,
            step: 2,
            sigma_scale_micro: 1_500_000,
            sharpness_micro: 7_000_000,
        },
        sample_count: 10,
        bias_millimeters: 70,
        ..settings()
    }
}
pub(super) fn camera(context: &crate::device::GpuContext, screen: &SceneMetadata) {
    camera_projection(
        context,
        screen,
        glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0),
    );
}
fn camera_projection(
    context: &crate::device::GpuContext,
    screen: &SceneMetadata,
    projection: glam::Mat4,
) {
    let mut values: Vec<_> = [glam::Mat4::IDENTITY, projection, projection.inverse()]
        .into_iter()
        .flat_map(|matrix| matrix.to_cols_array())
        .collect();
    values.extend([1.0, 1.0, 0.07, 0.0, 10.0, 7.0, 1.0, 0.0]);
    context.queue().write_buffer(
        &screen.uniform,
        0,
        &values
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
}
#[test]
fn gpu_scalable_taps_mips_and_response_match_reference() {
    let context = context();
    let screen = SceneMetadata::new(context.device(), [20, 2], recipe()).unwrap();
    camera(&context, &screen);
    let source = scalable::source(8);
    let prefix = source.split("@fragment").next().unwrap();
    let shader = format!(
        "{prefix}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ let i = u32(p.x); let v = tap_offset(i, 10u, phase(vec2<i32>(13, 17)), 2048.0); return vec4<f32>(v, f32(tap_level(length(v))), visibility(f32(i) * 0.1, 10u)); }}"
    );
    let pipeline = pipeline_format(context.device(), &shader, wgpu::TextureFormat::Rgba32Float);
    let group = group(context.device(), &pipeline, Some(&screen.uniform), &[]);
    let output = texture(context.device(), [10, 1], wgpu::TextureFormat::Rgba32Float);
    draw(&context, &pipeline, &group, &output);
    for (i, pixel) in read(&context, &output).chunks_exact(4).enumerate() {
        let index = f32::from(u16::try_from(i).unwrap());
        let rotation = (f32::from((13u16 * 18) ^ (140u16 * 145)) * 2.39996).fract();
        let t = (index + rotation + 0.5) * (1.0 / 10.0);
        let squared = t * t;
        let radius = f64::from(2048.0 * squared);
        let angle =
            f64::from(squared * (7.0 * std::f32::consts::TAU) + rotation * std::f32::consts::TAU);
        assert!(
            (f64::from(pixel[0]) - radius * angle.cos()).abs() < 0.01,
            "i={i} phase={rotation} actual={pixel:?} radius={radius} angle={angle}"
        );
        assert!((f64::from(pixel[1]) - radius * angle.sin()).abs() < 0.01);
        assert!((f64::from(pixel[2]) - (radius.log2().floor() - 3.0).clamp(0.0, 7.0)).abs() < 1e-6);
        assert!((f64::from(pixel[3]) - (1.0 - (f64::from(index) * 0.01).sqrt())).abs() < 1e-6);
    }
}
#[test]
fn gpu_scalable_plane_corner_and_full_resolution_normal() {
    plane_corner(false);
}
#[test]
fn gpu_scalable_orthographic_obscurance_matches_analytic_reference() {
    plane_corner(true);
}
fn plane_corner(orthographic: bool) {
    let context = context();
    let size = [33, 33];
    let screen = SceneMetadata::new(context.device(), [66, 66], recipe()).unwrap();
    let projection = if orthographic {
        glam::camera::rh::proj::directx::orthographic(-2.0, 2.0, -2.0, 2.0, 0.1, 100.0)
    } else {
        glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0)
    };
    camera_projection(&context, &screen, projection);
    let shader = scalable::source(1);
    let pipeline = pipeline_format(context.device(), &shader, wgpu::TextureFormat::Rg16Float);
    let output = texture(context.device(), size, wgpu::TextureFormat::Rg16Float);
    for corner in [false, true] {
        let values: Vec<_> = (0..1089)
            .map(|i| if corner && i % 33 > 16 { 1.5 } else { 2.0 })
            .collect();
        let depth = upload(&context, size, &values);
        for normal in [0x3c00u16, 0xbc00] {
            let normals = corner_normals(&context, normal);
            let group = group(
                context.device(),
                &pipeline,
                Some(&screen.uniform),
                &[&normals, &depth],
            );
            draw(&context, &pipeline, &group, &output);
            let centre = read(&context, &output)[(16 * 33 + 16) * 2];
            if corner && normal == 0x3c00 {
                assert!(centre < 0.95, "{centre}");
                assert!((f64::from(centre) - corner_reference(orthographic)).abs() < 0.001);
            } else {
                assert_eq!(centre.to_bits(), 1.0f32.to_bits());
            }
        }
    }
}
#[test]
fn gpu_scalable_filter_matches_gaussian_and_depth_key_reference() {
    let context = context();
    let size = [17, 1];
    let screen = SceneMetadata::new(context.device(), [34, 2], recipe()).unwrap();
    camera(&context, &screen);
    let depths = upload(&context, size, &[1.0; 17]);
    let input = texture(context.device(), size, wgpu::TextureFormat::Rg16Float);
    let values: Vec<f32> = (0..17)
        .flat_map(|i| {
            if i < 8 {
                [0.25, 0.015_625]
            } else {
                [0.75, 0.046_875]
            }
        })
        .collect();
    let words: Vec<u16> = (0..17)
        .flat_map(|i| {
            if i < 8 {
                [0x3400, 0x2400]
            } else {
                [0x3a00, 0x2a00]
            }
        })
        .collect();
    write_pairs(&context, &input, &words);
    let shader = scalable::blur_source(recipe().filter, [1, 0]);
    let pipeline = pipeline_format(context.device(), &shader, input.format());
    let group = group(
        context.device(),
        &pipeline,
        Some(&screen.uniform),
        &[&depths, &input],
    );
    let output = texture(context.device(), size, input.format());
    draw(&context, &pipeline, &group, &output);
    let actual = read(&context, &output);
    for x in 0..17 {
        let mut sum = f64::from(values[x * 2]);
        let mut total = 1.0;
        for off in -4i32..=4 {
            if off == 0 {
                continue;
            }
            let p = i32::try_from(x).unwrap() + off * 2;
            let sample = usize::try_from(p)
                .ok()
                .filter(|&p| p < 17)
                .map_or([0.0, 0.0], |p| [values[p * 2], values[p * 2 + 1]]);
            let difference = f64::from((values[x * 2 + 1] - sample[1]).abs());
            let weight = (-difference * 7.0 * 100.0
                - f64::from(off * off) / (2.0 * (4.0 / 1.5f64).powi(2)))
            .exp();
            sum += f64::from(sample[0]) * weight;
            total += weight;
        }
        assert!((f64::from(actual[x * 2]) - (1.01 * sum / total).min(1.0)).abs() < 0.001);
        assert_eq!(actual[x * 2 + 1].to_bits(), values[x * 2 + 1].to_bits());
    }
    assert!(actual[7 * 2] < 0.26);
    assert!(actual[8 * 2] > 0.75);
}
#[test]
fn gpu_scalable_hierarchy_odd_sizes_and_single_pixel_axes() {
    let context = context();
    for size in [[1, 1], [1, 7], [7, 1], [5, 3], [513, 257]] {
        let screen = SceneMetadata::new(context.device(), size, recipe()).unwrap();
        assert_eq!(
            [
                screen.passes.filtered.width(),
                screen.passes.filtered.height()
            ],
            size.map(|s| s.div_ceil(2))
        );
        let levels: Vec<_> = screen
            .passes
            .stages
            .iter()
            .filter(|s| matches!(s.slot, PassSlot::OcclusionMetadata(_)))
            .collect();
        assert!(levels.len() <= 8);
        assert_eq!(
            [
                levels.last().unwrap().target.width(),
                levels.last().unwrap().target.height()
            ],
            [size[0].div_ceil(256).max(1), size[1].div_ceil(256).max(1)]
        );
    }
    let source = upload(
        &context,
        [5, 3],
        &[
            0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0,
        ],
    );
    let output = texture(context.device(), [3, 2], wgpu::TextureFormat::R32Float);
    let pipeline = pipeline(context.device(), include_str!("mean_depth.wgsl"));
    let group = group(context.device(), &pipeline, None, &[&source]);
    draw(&context, &pipeline, &group, &output);
    // Exact on every conforming device: the inputs are even integers below 2^24, so each
    // four-texel sum is an integer and multiplying by 0.25 gives a multiple of 0.25. All of
    // them are representable, so correctly rounded addition and multiplication return them
    // unchanged; R32Float readback is a transfer.
    assert_eq!(read(&context, &output), [6.0, 10.0, 13.0, 21.0, 25.0, 28.0]);
}

fn write_pairs(context: &crate::device::GpuContext, texture: &wgpu::Texture, words: &[u16]) {
    context.queue().write_texture(
        texture.as_image_copy(),
        &words
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 4),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}

fn corner_reference(orthographic: bool) -> f64 {
    let scale = if orthographic {
        0.5
    } else {
        f64::from(
            glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0)
                .y_axis
                .y,
        )
    };
    let rotation = f64::from((f32::from((16u16 * 17) ^ (143u16 * 144)) * 2.39996).fract());
    let radius = scale * 33.0 * 0.5 / if orthographic { 1.0 } else { 2.0 };
    let mut sum = 0.0;
    for index in 0u32..10 {
        let t = ((f64::from(index) + rotation + 0.5) / 10.0).powi(2);
        let angle = (t * 7.0 + rotation) * std::f64::consts::TAU;
        let pixel = [
            16.5 + radius * t * angle.cos(),
            16.5 + radius * t * angle.sin(),
        ];
        let uv = pixel.map(|p| (p / 33.0).min(1.0 - 2.0 / 33.0));
        let x = (uv[0] * 33.0 - 0.5).clamp(0.0, 32.0);
        let depth = if x <= 16.0 {
            2.0
        } else if x >= 17.0 {
            1.5
        } else {
            2.0 - 0.5 * (x - 16.0)
        };
        let lateral_depth = if orthographic { 1.0 } else { depth };
        let delta = [
            (2.0 * pixel[0] / 33.0 - 1.0) * lateral_depth / scale,
            (1.0 - 2.0 * pixel[1] / 33.0) * lateral_depth / scale,
            2.0 - depth,
        ];
        let squared = delta.iter().map(|v| v * v).sum::<f64>();
        let f = (1.0 - squared).clamp(0.0, 1.0) * (delta[2] - 0.07).clamp(0.0, 1.0)
            / (0.0002 + squared);
        sum += f * (f * 0.05 + 1.4 * 0.95);
    }
    (1.0 - (sum / 10.0).sqrt()).clamp(0.0, 1.0)
}
