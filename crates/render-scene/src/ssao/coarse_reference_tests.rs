use super::*;
use crate::frame::FrameUniform;
use crate::scene_metadata::SceneMetadata;
use crate::ssao::tests::{context, draw, settings};
use worldrunner_render_plan::SceneSsaoKernel;
use worldrunner_render_post::PrepareContext;

fn reference(
    pixel: [u32; 2],
    levels: &[Vec<f32>],
    settings: worldrunner_render_plan::SceneSsao,
    selected: &mut [bool; 4],
) -> f32 {
    const SIDE: u32 = 769;
    let projection = 1.0 / 0.5f32.tan();
    let depth = levels[0][usize::try_from(pixel[1] * SIDE + pixel[0]).unwrap()];
    let point = |p: [f32; 2], d: f32| {
        glam::Vec3::new(
            (2.0 * p[0] / 769.0 - 1.0) * d / projection,
            (1.0 - 2.0 * p[1] / 769.0) * d / projection,
            -d,
        )
    };
    let centre = point(
        pixel.map(|v| crate::ssao::scalar(u64::from(v)) + 0.5),
        depth,
    );
    let radius_metres = crate::ssao::scalar(settings.radius_millimeters) / 1000.0;
    let radius_pixels = projection * radius_metres / depth * 769.0 * 0.5;
    let rotation = (crate::ssao::scalar(u64::from(pixel[0])) * 0.067_110_56
        + crate::ssao::scalar(u64::from(pixel[1])) * 0.005_837_15)
        .fract()
        * std::f32::consts::TAU;
    let mut sum = 0.0;
    for i in 0..settings.sample_count {
        let t = (crate::ssao::scalar(u64::from(i)) + 0.5)
            / crate::ssao::scalar(u64::from(settings.sample_count));
        let radius = (radius_pixels * t * t).max(1.0);
        let angle = crate::ssao::scalar(u64::from(i)) * 2.399_963_1 + rotation;
        let offset = (glam::Vec2::new(angle.cos(), angle.sin()) * radius)
            .round()
            .as_ivec2();
        let p = [
            i32::try_from(pixel[0]).unwrap() + offset.x,
            i32::try_from(pixel[1]).unwrap() + offset.y,
        ];
        if p.iter().any(|&v| !(0..769).contains(&v)) {
            continue;
        }
        let binding = if radius >= 256.0 {
            3
        } else if radius >= 64.0 {
            2
        } else {
            usize::from(radius >= 16.0)
        };
        selected[binding] = true;
        let level = binding * 2;
        let scale = 1u32 << level;
        let side = SIDE.div_ceil(scale);
        let p = p.map(|v| u32::try_from(v).unwrap());
        let sample = levels[level][usize::try_from(p[1] / scale * side + p[0] / scale).unwrap()];
        let delta = point(p.map(|v| crate::ssao::scalar(u64::from(v)) + 0.5), sample) - centre;
        let length = delta.length();
        let cosine = delta.z / length;
        if cosine <= std::f32::consts::FRAC_PI_2.cos() {
            continue;
        }
        let falloff = (1.0 - length * length / (radius_metres * radius_metres)).max(0.0);
        sum += falloff * (cosine - 0.01 / length).max(0.0);
    }
    (1.0 - sum / crate::ssao::scalar(u64::from(settings.sample_count))).clamp(0.0, 1.0)
}

#[test]
fn gpu_production_spiral_samples_distinct_reduced_levels_against_cpu() {
    let context = context();
    let settings = worldrunner_render_plan::SceneSsao {
        kernel: SceneSsaoKernel::Spiral,
        radius_millimeters: 3000,
        ..settings()
    };
    let screen = SceneMetadata::new(context.device(), [769, 769], settings).unwrap();
    prepare(&context, &screen);
    let mut selected = [false; 4];
    for shape in 0..3 {
        let levels = hierarchy(shape);
        let actual = render(&context, &screen, &levels);
        for pixel in [[383, 383], [300, 300], [400, 400], [420, 350], [350, 420]] {
            let expected = reference(pixel, &levels, settings, &mut selected);
            let value = actual[usize::try_from(pixel[1] * 769 + pixel[0]).unwrap()];
            assert!(
                (value - expected).abs() < 2e-5,
                "shape {shape}, pixel {pixel:?}: {value} != {expected}"
            );
            if shape == 1 && pixel == [383, 383] {
                assert!(expected < 0.99);
            }
        }
    }
    assert_eq!(
        selected, [true; 4],
        "every production binding must be sampled"
    );
}

fn prepare(context: &crate::device::GpuContext, screen: &SceneMetadata) {
    let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0);
    let frame = FrameUniform {
        view: glam::Mat4::IDENTITY,
        view_projection: projection,
        camera_position: glam::Vec3::ZERO,
        camera_position_millimeters: [0; 3],
        sun_direction: glam::Vec3::Z,
        sun_irradiance: glam::Vec3::ZERO,
        ambient_sky: glam::Vec3::ONE,
        ambient_ground: glam::Vec3::ONE,
        exposure: 1.0,
        unlit: false,
        animation_time_milliseconds: 0,
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    screen.upload(&mut prepare, frame);
    let _uploads = prepare.finish();
    encoder.finish().submit().unwrap();
    let normals: Vec<_> = (0..769 * 769)
        .flat_map(|_| [0u16, 0, 0x3c00, 0x3c00])
        .flat_map(u16::to_le_bytes)
        .collect();
    context.queue().write_texture(
        screen.normals.as_image_copy(),
        &normals,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(769 * 8),
            rows_per_image: Some(769),
        },
        screen.normals.size(),
    );
}

fn hierarchy(shape: u32) -> Vec<Vec<f32>> {
    let depths: Vec<f32> = (0..769 * 769)
        .map(|i| match shape {
            1 if i % 769 >= 384 && i / 769 >= 384 => 1.5,
            2 if i % 769 >= 384 => 4.0,
            _ => 2.0,
        })
        .collect();
    // Splits align with every sampled footprint, so CPU means remain exact binary values.
    let mut levels = vec![depths];
    let mut side = 769u32;
    for _ in 1..=6 {
        let next = side.div_ceil(2);
        let source = levels.last().unwrap();
        let mut reduced = Vec::new();
        for y in 0..next {
            for x in 0..next {
                let mut sum = 0.0;
                for dy in 0..2 {
                    for dx in 0..2 {
                        sum += source[usize::try_from(
                            (2 * y + dy).min(side - 1) * side + (2 * x + dx).min(side - 1),
                        )
                        .unwrap()];
                    }
                }
                reduced.push(sum / 4.0);
            }
        }
        levels.push(reduced);
        side = next;
    }
    levels
}

fn render(
    context: &crate::device::GpuContext,
    screen: &SceneMetadata,
    levels: &[Vec<f32>],
) -> Vec<f32> {
    let linear = &screen.passes.stages[0].target;
    context.queue().write_texture(
        linear.as_image_copy(),
        &levels[0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(769 * 4),
            rows_per_image: Some(769),
        },
        linear.size(),
    );
    for stage in &screen.passes.stages {
        if matches!(stage.slot, PassSlot::OcclusionMetadata(_) | PassSlot::Ssao) {
            draw(context, &stage.pipeline, &stage.group, &stage.target);
        }
    }
    let raw = &screen
        .passes
        .stages
        .iter()
        .find(|s| s.slot == PassSlot::Ssao)
        .unwrap()
        .target;
    let bytes = worldrunner_render_post::Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new([769, 769], raw.format(), 1),
        3_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), raw)
    .unwrap();
    bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect()
}
