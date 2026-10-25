use super::*;
use crate::ssao::tests::{context, draw, read, settings, upload};
use worldrunner_render_plan::{SceneSsaoFilter, SceneSsaoKernel};
use worldrunner_render_post::{FrameConfig, Readback};

fn bilateral_cpu(size: [u32; 2], depth: &[f32], input: &[f64], axis: [i32; 2]) -> Vec<f64> {
    let [width, height] = size.map(|v| i32::try_from(v).unwrap());
    (0..width * height)
        .map(|index| {
            let (x, y) = (index % width, index / width);
            let centre = usize::try_from(index).unwrap();
            let mut sum = input[centre];
            let mut total = 1.0;
            for tap in [-4, -3, -2, -1, 1, 2, 3, 4] {
                let px = (x + axis[0] * tap * 2).clamp(0, width - 1);
                let py = (y + axis[1] * tap * 2).clamp(0, height - 1);
                let source = usize::try_from(py * width + px).unwrap();
                let difference = f64::from((depth[source] - depth[centre]).abs());
                if difference < 1.0 {
                    // sigma = 8/3, so the spatial exponent is -9*tap^2/128.
                    let weight = (-7.0 * difference - 9.0 * f64::from(tap * tap) / 128.0).exp();
                    sum += input[source] * weight;
                    total += weight;
                }
            }
            sum / total
        })
        .collect()
}

fn close(actual: &[f32], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        // R32Float avoids colour quantization; this allows only f32 exp/accumulation error.
        assert!(
            (f64::from(*actual) - expected).abs() < 2e-6,
            "pixel {index}: {actual} != {expected}"
        );
    }
}

#[test]
fn gpu_bilateral_weights_match_cpu_and_production_two_axis_wiring() {
    let context = context();
    let size = [11, 9];
    let depths: Vec<_> = (0..99)
        .map(|i| 2.0 + f32::from(u16::try_from((i % 11 * 3 + i / 11 * 5) % 9).unwrap()) * 0.125)
        .collect();
    let values: Vec<_> = (0..99)
        .map(|i| f32::from(u16::try_from((i * 13 + 7) % 31).unwrap()) / 32.0)
        .collect();
    let reference_values: Vec<_> = values.iter().copied().map(f64::from).collect();
    let depth = upload(&context, size, &depths);
    let raw = upload(&context, size, &values);
    for axis in [[1, 0], [0, 1]] {
        let pipeline = pipeline(context.device(), &crate::ssao::bilateral::source(axis));
        let group = group(context.device(), &pipeline, None, &[&depth, &raw]);
        let target = texture(context.device(), size, wgpu::TextureFormat::R32Float);
        draw(&context, &pipeline, &group, &target);
        close(
            &read(&context, &target),
            &bilateral_cpu(size, &depths, &reference_values, axis),
        );
    }
    let screen =
        crate::scene_metadata::SceneMetadata::new(context.device(), size, settings()).unwrap();
    let mut stages = Vec::new();
    let filtered = kernels::append_filter(
        context.device(),
        &screen.uniform,
        &depth,
        raw,
        SceneSsaoFilter::SeparableBilateral,
        &mut stages,
    );
    assert_eq!(
        stages.iter().map(|s| s.slot).collect::<Vec<_>>(),
        [PassSlot::SsaoBlurHorizontal, PassSlot::SsaoBlur]
    );
    for stage in stages {
        draw(&context, &stage.pipeline, &stage.group, &stage.target);
    }
    let horizontal = bilateral_cpu(size, &depths, &reference_values, [1, 0]);
    close(
        &read(&context, &filtered),
        &bilateral_cpu(size, &depths, &horizontal, [0, 1]),
    );
}

fn half(value: i16) -> u16 {
    match value {
        -8 => 0xc800,
        -4 => 0xc400,
        -2 => 0xc000,
        -1 => 0xbc00,
        0 => 0,
        1 => 0x3c00,
        2 => 0x4000,
        4 => 0x4400,
        8 => 0x4800,
        _ => panic!("fixture value"),
    }
}
fn metadata(
    context: &crate::device::GpuContext,
    size: [u32; 2],
    values: &[[u16; 4]],
) -> wgpu::Texture {
    let source = texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    context.queue().write_texture(
        source.as_image_copy(),
        &values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 8),
            rows_per_image: Some(size[1]),
        },
        source.size(),
    );
    source
}

#[test]
fn gpu_average_normals_coverage_and_both_odd_edges_are_exact_half_float() {
    let context = context();
    // Each footprint has four covered inputs or two covered and two uncovered.
    // All means are binary fractions, including opposing normals and replicated edges.
    let values: [[i16; 4]; 15] = [
        [1, 0, 0, 2],
        [-1, 0, 0, 4],
        [0, 1, 0, 2],
        [0, -1, 0, 2],
        [1, 0, 0, 8],
        [0, 1, 0, 2],
        [0, 0, 1, 8],
        [8, 8, 8, 0],
        [4, 4, 4, 0],
        [-1, 0, 0, 4],
        [0, 0, 1, 4],
        [0, 0, -1, 8],
        [2, 2, 2, 0],
        [4, 4, 4, 0],
        [0, 1, 0, 2],
    ];
    let source = metadata(&context, [5, 3], &values.map(|v| v.map(half)));
    let output = texture(context.device(), [3, 2], wgpu::TextureFormat::Rgba16Float);
    let pipeline = pipeline_format(
        context.device(),
        include_str!("average.wgsl"),
        output.format(),
    );
    let group = group(context.device(), &pipeline, None, &[&source]);
    draw(&context, &pipeline, &group, &output);
    let bytes = Readback::new(
        context.device(),
        FrameConfig::new([3, 2], output.format(), 1),
        1024,
    )
    .unwrap()
    .read(context.device(), context.submission(), &output)
    .unwrap();
    let actual: Vec<_> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
        .collect();
    // Means: (0,1/4,1/4,4), (0,0,0,2), (0,0,0,6),
    // (0,0,0,6), uncovered, (0,1,0,2).
    assert_eq!(
        actual,
        [
            0, 0x3400, 0x3400, 0x4400, 0, 0, 0, 0x4000, 0, 0, 0, 0x4600, 0, 0, 0, 0x4600, 0, 0, 0,
            0, 0, 0x3c00, 0, 0x4000
        ]
    );
}

#[test]
fn averaged_hierarchy_stops_at_the_highest_sampled_level() {
    let context = context();
    let screen = crate::scene_metadata::SceneMetadata::new(
        context.device(),
        [257, 129],
        worldrunner_render_plan::SceneSsao {
            kernel: SceneSsaoKernel::Spiral,
            ..settings()
        },
    )
    .unwrap();
    let levels: Vec<_> = screen
        .passes
        .stages
        .iter()
        .filter_map(|s| match s.slot {
            PassSlot::OcclusionMetadata(level) => Some(level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, (0..=6).collect::<Vec<_>>());
}
