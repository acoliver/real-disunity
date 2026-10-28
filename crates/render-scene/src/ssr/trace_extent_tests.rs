use super::*;
use worldrunner_render_plan::{SceneDownsampleRounding, SceneSsrTraversal};

fn probe_source(source: &str) -> String {
    let declarations = source.split("@group(0) @binding(1)").next().unwrap();
    let fragment = source.split("@fragment fn fs_screen").nth(1).unwrap();
    let coordinates = fragment.split("    let p = clamp").next().unwrap();
    format!(
        "{declarations}\n@fragment fn fs_screen{coordinates}
        return vec4<f32>(f32(sample), origin_uv, 1.0);
    }}"
    )
}

fn probe(context: &GpuContext, config: FrameConfig, policy: &SceneSsr, source: &str) -> Vec<u8> {
    let pipeline = gpu::pipeline(context.device(), &probe_source(source), 1, false);
    let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("trace extent oracle"),
        size: 208,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut values = [0.0f32; 52];
    values[36] = gpu::scalar(config.size[0]);
    values[37] = gpu::scalar(config.size[1]);
    values[38] = 2.0;
    context.queue().write_buffer(
        &uniform,
        0,
        &values
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let group = gpu::group(context.device(), &pipeline, Some(&uniform), &[]);
    let targets = gpu::radiance_textures(context.device(), config, policy);
    application_policy_tests::draw_policy(context, &pipeline, &group, &targets[0]);
    read(context, &targets[0])
}

fn assert_coordinates(bytes: &[u8], extent: [u32; 2], samples: u32) {
    let [width, height] = extent;
    for (index, pixel) in bytes.chunks_exact(8).enumerate() {
        let index = u32::try_from(index).unwrap();
        let x = index % (width * samples);
        let y = index / (width * samples);
        let expected = [
            gpu::scalar(x / width),
            (gpu::scalar(x % width) + 0.5) / gpu::scalar(width),
            (gpu::scalar(y) + 0.5) / gpu::scalar(height),
            1.0,
        ];
        assert_pixel(pixel, expected);
    }
}

fn assert_pixel(pixel: &[u8], expected: [f32; 4]) {
    for (channel, value) in expected.into_iter().enumerate() {
        let actual = u16::from_le_bytes(pixel[channel * 2..channel * 2 + 2].try_into().unwrap());
        // GPU half-float attachment conversion can differ from CPU round-to-nearest by one ULP.
        let tolerance = u16::from(channel == 1 || channel == 2);
        assert!(
            actual.abs_diff(half(value)) <= tolerance,
            "channel {channel}: {actual} != {}",
            half(value)
        );
    }
}

fn check_floor(context: &GpuContext, samples: u32, traversal: SceneSsrTraversal, water: bool) {
    let policy = SceneSsr {
        resolution_scale_micro: 500_000,
        radiance_rounding: SceneDownsampleRounding::Floor,
        traversal,
        ..settings()
    };
    for (size, extent) in [([7, 5], [3, 2]), ([1, 7], [1, 3]), ([7, 1], [3, 1])] {
        let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
        let source = grid::trace_source(samples, water, &policy, size);
        let bytes = probe(context, config, &policy, &source);
        assert_eq!(
            bytes.len(),
            usize::try_from(extent[0] * extent[1] * samples * 8).unwrap()
        );
        assert_coordinates(&bytes, extent, samples);
        if size == [7, 5] && samples == 4 {
            let lanes: Vec<_> = bytes[..12 * 8]
                .chunks_exact(8)
                .map(|pixel| u16::from_le_bytes([pixel[0], pixel[1]]))
                .collect();
            assert_eq!(
                lanes,
                [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0].map(half)
            );
            assert_pixel(&bytes[..8], [0.0, 1.0 / 6.0, 0.25, 1.0]);
        }
    }
}

#[test]
fn gpu_floor_trace_lanes_and_origins_match_allocated_extents() {
    let context = context();
    for samples in [1, 4] {
        for traversal in [SceneSsrTraversal::Conservative, SceneSsrTraversal::Grid] {
            for water in [false, true] {
                check_floor(&context, samples, traversal, water);
            }
        }
    }
}

#[test]
fn gpu_default_trace_coordinates_preserve_legacy_bytes() {
    let context = context();
    let policy = SceneSsr {
        resolution_scale_micro: 500_000,
        ..settings()
    };
    for samples in [1, 4] {
        for water in [false, true] {
            let legacy = gpu::trace_source(samples, water);
            assert_eq!(
                gpu::configured_trace_source(samples, water, SceneDownsampleRounding::Ceil),
                legacy
            );
            let config = FrameConfig::new([7, 5], wgpu::TextureFormat::Rgba16Float, samples);
            let expected = probe(&context, config, &policy, &legacy);
            assert_coordinates(&expected, [4, 3], samples);
            for traversal in [SceneSsrTraversal::Conservative, SceneSsrTraversal::Grid] {
                let selected = SceneSsr {
                    traversal,
                    ..policy
                };
                let source = grid::trace_source(samples, water, &selected, config.size);
                assert_eq!(probe(&context, config, &selected, &source), expected);
            }
        }
    }
}
