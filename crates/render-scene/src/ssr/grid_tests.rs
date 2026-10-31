#[path = "grid_review_tests.rs"]
mod review_tests;
use std::fmt::Write;

#[test]
fn gpu_grid_binds_complete_chains_within_device_limits() {
    let context = context();
    for (size, count) in [([65, 33], 8), ([257, 129], 10)] {
        let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
        let result = Ssr::new(
            context.device(),
            FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 4),
            SceneSsr {
                traversal: SceneSsrTraversal::Grid,
                grid_level_count: Some(count),
                ..settings()
            },
            &mut screen,
        );
        if count + 8
            <= context
                .device()
                .limits()
                .max_sampled_textures_per_shader_stage
        {
            let ssr = result.unwrap();
            assert_eq!(ssr.hierarchy.len(), usize::try_from(count).unwrap());
            assert_eq!(ssr.hierarchy.last().unwrap().width(), 1);
        } else {
            assert!(matches!(
                result,
                Err(FrameError::SsrSettings(
                    "grid levels exceed device texture binding limit"
                ))
            ));
        }
    }
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}

#[test]
fn gpu_grid_half_output_domain_produces_reflections() {
    let context = context();
    let policy = SceneSsr {
        traversal: SceneSsrTraversal::Grid,
        grid_domain: worldrunner_render_plan::SceneSsrGridDomain::ReflectionOutput,
        resolution_scale_micro: 500_000,
        grid_level_count: Some(7),
        ..settings()
    };
    let bytes = controls_tests::capture_with_shader(&context, policy, 1.0, false, None);
    assert!(bytes.chunks_exact(8).any(|pixel| pixel[6..8] != [0, 0]));
}

#[test]
fn gpu_grid_long_hit_requires_more_than_the_old_step_limit() {
    let context = context();
    let depths = pyramid(
        (0..512 * 512)
            .map(|i| if i % 512 < 256 { 1.0 } else { 3000.0 })
            .collect(),
    );
    let ray = Ray {
        a: [2.25, 256.25],
        b: [509.25, 256.25],
        weighted: [1000.0, 4000.0],
        reciprocal: [1.0, 1.0],
        skip: 0.0,
        budget: 1024,
        behind: 256,
        base: 0,
        side: 512,
        levels: 10,
    };
    let expected = oracle(ray, &depths);
    assert!(expected[5] > 0.5);
    assert!(expected[4] > 256.0);
    assert!(oracle(Ray { budget: 256, ..ray }, &depths)[5] < 0.5);
    let actual = capture_grid(&context, ray, &depths);
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() < 0.25, "{actual} != {expected}");
    }
}

#[test]
fn gpu_grid_output_domain_full_chain_and_long_distance_match_cpu() {
    let context = context();
    for base in [0, 1] {
        let depths = pyramid(vec![2500.0; 1024]);
        let depths = &depths[usize::try_from(base).unwrap()..];
        let divisor = f64::from(1u32 << base);
        let ray = Ray {
            a: [2.25 / divisor, 16.25 / divisor],
            b: [29.25 / divisor, 16.25 / divisor],
            weighted: [1000.0, 4000.0],
            reciprocal: [1.0, 1.0],
            skip: 0.0,
            budget: 1024,
            behind: 0,
            base,
            side: 32,
            levels: 6 - base,
        };
        let expected = oracle(ray, depths);
        assert!(expected[5] > 0.5);
        assert!(expected[2] > 1000.0);
        let actual = capture_grid(&context, ray, depths);
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.025,
                "{ray:?}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn grid_domain_validation_rejects_unavailable_levels_and_accepts_limits() {
    let policy = SceneSsr {
        traversal: SceneSsrTraversal::Grid,
        grid_domain: worldrunner_render_plan::SceneSsrGridDomain::ReflectionOutput,
        resolution_scale_micro: 500_000,
        grid_level_count: Some(5),
        max_distance_millimeters: 4_000_000,
        max_steps: 1024,
        ..settings()
    };
    assert!(policy.validate().is_ok());
    let limits = wgpu::Limits::default();
    assert_eq!(grid::domain(&policy, [32, 32]), (1, 5));
    assert!(grid::validate([32, 32], &policy, &limits).is_ok());
    assert!(grid::validate([16, 16], &policy, &limits).is_err());
    assert!(grid::validate([1, 1], &policy, &limits).is_err());
    assert!(
        grid::validate(
            [32, 32],
            &SceneSsr {
                grid_level_count: Some(6),
                ..policy
            },
            &limits
        )
        .is_err()
    );
}

#[test]
fn gpu_grid_output_domain_compiles_odd_extents_and_water_msaa() {
    let context = context();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let size = [33, 25];
        let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
        let ssr = Ssr::new(
            context.device(),
            FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples),
            SceneSsr {
                traversal: SceneSsrTraversal::Grid,
                grid_domain: worldrunner_render_plan::SceneSsrGridDomain::ReflectionOutput,
                resolution_scale_micro: 500_000,
                grid_level_count: Some(6),
                ..settings()
            },
            &mut screen,
        )
        .unwrap();
        assert_eq!(
            ssr.hierarchy
                .iter()
                .map(wgpu::Texture::width)
                .collect::<Vec<_>>(),
            [17, 9, 5, 3, 2, 1, 33]
        );
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
}

use super::*;
use worldrunner_render_plan::SceneSsrTraversal;

#[derive(Clone, Copy, Debug)]
struct Ray {
    a: [f64; 2],
    b: [f64; 2],
    weighted: [f64; 2],
    reciprocal: [f64; 2],
    skip: f64,
    budget: u32,
    behind: u32,
    base: u32,
    side: u32,
    levels: u32,
}

fn pyramid(depths: Vec<f64>) -> Vec<Vec<f64>> {
    let mut levels = vec![depths];
    let mut side = levels[0].len().isqrt();
    while side > 1 {
        let previous = levels.last().unwrap();
        let mut next = Vec::new();
        for y in 0..side / 2 {
            for x in 0..side / 2 {
                next.push(
                    (previous[(y * 2) * side + x * 2]
                        + previous[(y * 2) * side + x * 2 + 1]
                        + previous[(y * 2 + 1) * side + x * 2]
                        + previous[(y * 2 + 1) * side + x * 2 + 1])
                        / 4.0,
                );
            }
        }
        levels.push(next);
        side /= 2;
    }
    levels
}

impl Ray {
    fn position(self, axis: f64, primary: usize) -> [f64; 3] {
        let t = (axis - self.a[primary]) / (self.b[primary] - self.a[primary]);
        [
            self.a[0] + t * (self.b[0] - self.a[0]),
            self.a[1] + t * (self.b[1] - self.a[1]),
            (self.weighted[0] + t * (self.weighted[1] - self.weighted[0]))
                / (self.reciprocal[0] + t * (self.reciprocal[1] - self.reciprocal[0])),
        ]
    }
    fn end(self) -> f64 {
        let mut end = 1.0f64;
        for (axis, delta) in [self.b[0] - self.a[0], self.b[1] - self.a[1]]
            .into_iter()
            .enumerate()
        {
            if delta != 0.0 {
                let boundary = if delta > 0.0 {
                    f64::from(self.side) / f64::from(1u32 << self.base)
                } else {
                    0.0
                };
                end = end.min((boundary - self.a[axis]) / delta);
            }
        }
        end
    }
}
fn fetch(depths: &[Vec<f64>], level: usize, point: [f64; 3]) -> f64 {
    let width = f64::from(1u32 << level);
    let side = i32::try_from(depths[level].len().isqrt()).unwrap();
    let coordinate = (glam::DVec2::new(point[0], point[1]) / width)
        .as_ivec2()
        .clamp(glam::IVec2::ZERO, glam::IVec2::splat(side - 1));
    depths[level][usize::try_from(coordinate.y * side + coordinate.x).unwrap()]
}
// Parameter-space oracle: reconstruct each endpoint independently in double precision.
fn oracle(ray: Ray, depths: &[Vec<f64>]) -> [f64; 8] {
    let primary = usize::from((ray.b[1] - ray.a[1]).abs() > (ray.b[0] - ray.a[0]).abs());
    let delta = ray.b[primary] - ray.a[primary];
    let direction = delta.signum();
    let offset = f64::from(u8::from(direction > 0.0));
    let stop = ((ray.a[primary] + ray.end() * delta) * direction).floor() + offset;
    let mut axis = (ray.a[primary] + ray.skip * direction).floor() + offset;
    let (mut level, mut levels, mut steps) = (0usize, 0u32, 0u32);
    let mut allowance = ray.behind;
    while axis * direction < stop && steps < ray.budget {
        steps += 1;
        levels = (levels * 5 + u32::try_from(level).unwrap() + 1) % 1021;
        let width = f64::from(1u32 << level);
        let next = ((((axis + direction * 0.5) / width).floor() + offset) * width * direction)
            .min(stop)
            * direction;
        let mut start = ray.position(axis, primary);
        let mut finish = ray.position(next, primary);
        start[primary] += direction * 0.001;
        finish[primary] -= direction * 0.001;
        let surface = fetch(depths, level, start).min(fetch(depths, level, finish));
        if surface < start[2].max(finish[2]) {
            if level > 0 {
                level -= 1;
                continue;
            }
            if start[2].min(finish[2]) - 0.25 <= surface {
                return oracle_hit(
                    ray,
                    ray.position(axis + direction * 0.5, primary),
                    surface,
                    levels,
                    steps,
                );
            }
            if allowance == 0 {
                break;
            }
            allowance -= 1;
        } else {
            level = (level + 1).min(usize::try_from(ray.levels - 1).unwrap());
        }
        axis = next;
    }
    oracle_miss(levels, steps)
}

fn oracle_hit(ray: Ray, hit: [f64; 3], surface: f64, levels: u32, steps: u32) -> [f64; 8] {
    let scale = f64::from(1u32 << ray.base);
    if hit[..2]
        .iter()
        .any(|&v| v < 0.0 || v * scale >= f64::from(ray.side))
    {
        return oracle_miss(levels, steps);
    }
    [
        hit[0],
        hit[1],
        surface,
        f64::from(levels),
        f64::from(steps),
        1.0,
        (hit[0] * scale).trunc(),
        (hit[1] * scale).trunc(),
    ]
}

fn oracle_miss(levels: u32, steps: u32) -> [f64; 8] {
    [
        0.0,
        0.0,
        -1.0,
        f64::from(levels),
        f64::from(steps),
        0.0,
        0.0,
        0.0,
    ]
}

fn grid_shader(ray: Ray) -> String {
    let full = grid::trace_source(
        1,
        false,
        &SceneSsr {
            traversal: SceneSsrTraversal::Grid,
            grid_domain: worldrunner_render_plan::SceneSsrGridDomain::ReflectionOutput,
            resolution_scale_micro: if ray.base == 1 { 500_000 } else { 1_000_000 },
            grid_level_count: Some(ray.levels),
            ..settings()
        },
        [ray.side; 2],
    );
    let helpers = full.split("struct GridIntersection").nth(1).unwrap();
    let helpers = helpers.split("fn grid_reflection").next().unwrap();
    let mut bindings = String::new();
    for i in 0..ray.levels {
        writeln!(
            bindings,
            "@group(0) @binding({i}) var grid_depth_{i}: texture_2d<f32>;"
        )
        .unwrap();
    }
    let projection = full.split("    let grid_size =").nth(1).unwrap();
    let projection = projection.split("    let weighted =").next().unwrap();
    let mapping = full.split("    if intersection.depth <").nth(1).unwrap();
    let mapping = mapping.split("    let hit_normal =").next().unwrap();
    let mapping = format!("if intersection.depth <{mapping}")
        .replace("return vec4<f32>(0.0);", "intersection.depth = -1.0;");
    let scale = f64::from(1u32 << ray.base);
    let clip = |v: f64| v * scale / f64::from(ray.side) * 2.0 - 1.0;
    format!("{bindings}\nstruct GridIntersection{helpers}
        @fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {{
        let camera = vec4<f32>({}.0, {}.0, {}, 0.0);
        let clip_a = vec4<f32>({}, {}, 0.0, 1.0);
        let clip_b = vec4<f32>({}, {}, 0.0, 1.0);
        let reciprocal = vec2<f32>(1.0);
        let grid_size ={projection}
        var intersection = grid_walk(first, last, vec2<f32>({}, {}),
            vec2<f32>({}, {}), 1.0, grid_size, {}, {}u, vec2<f32>(0.25, 0.0), {}u);
        {mapping}
        if pixel.x > 1.0 {{ return vec4<f32>(f32(intersection.steps), select(0.0, 1.0, intersection.depth >= 0.0), select(vec2<f32>(0.0), vec2<f32>(hit_pixel), intersection.depth >= 0.0)); }}
        return vec4<f32>(select(vec2<f32>(0.0), intersection.pixel, intersection.depth >= 0.0), intersection.depth, f32(intersection.levels)); }}",
        ray.side, ray.side, scale, clip(ray.a[0]), -clip(ray.a[1]),
        clip(ray.b[0]), -clip(ray.b[1]), ray.weighted[0], ray.weighted[1],
        ray.reciprocal[0], ray.reciprocal[1], ray.skip, ray.budget, ray.behind)
        .replace("camera.size", "camera")
}
fn capture_grid(context: &GpuContext, ray: Ray, levels: &[Vec<f64>]) -> Vec<f64> {
    let pipeline = gpu::pipeline(context.device(), &grid_shader(ray), 1, false);
    let textures = grid_textures(context, &levels[..usize::try_from(ray.levels).unwrap()]);
    let views: Vec<_> = textures
        .iter()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let entries: Vec<_> = views
        .iter()
        .enumerate()
        .map(|(i, view)| wgpu::BindGroupEntry {
            binding: u32::try_from(i).unwrap(),
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    let group = context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
    let target =
        crate::ssao::passes::texture(context.device(), [2, 1], wgpu::TextureFormat::Rgba16Float);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    read(context, &target)
        .chunks_exact(2)
        .take(8)
        .map(|v| {
            f64::from(half::f16::from_bits(u16::from_le_bytes(v.try_into().unwrap())).to_f32())
        })
        .collect()
}
fn grid_textures(context: &GpuContext, levels: &[Vec<f64>]) -> Vec<wgpu::Texture> {
    levels
        .iter()
        .map(|values| {
            let side = u32::try_from(values.len().isqrt()).unwrap();
            let texture = crate::ssao::passes::texture(
                context.device(),
                [side, side],
                wgpu::TextureFormat::R32Float,
            );
            upload(
                context,
                &texture,
                &values
                    .iter()
                    .map(|v| half::f16::from_f64(*v).to_f32())
                    .collect::<Vec<_>>(),
            );
            texture
        })
        .collect()
}

fn shape_depths(shape: u32) -> Vec<Vec<f64>> {
    pyramid(
        (0..1024)
            .map(|i| match shape {
                0 => 5.0,
                1 => {
                    if i % 32 >= 16 {
                        4.75
                    } else {
                        9.0
                    }
                }
                2 => {
                    if i % 32 == 14 {
                        1.0
                    } else {
                        5.0
                    }
                }
                _ => 20.0,
            })
            .collect(),
    )
}

#[test]
fn gpu_grid_positions_misses_and_mip_selection_match_independent_cpu() {
    let context = context();
    let mut hits = 0;
    let mut misses = 0;
    for (a, b) in [
        ([2.25, 8.25], [29.25, 20.25]),
        ([29.25, 8.25], [2.25, 20.25]),
        ([8.25, 2.25], [20.25, 29.25]),
        ([8.25, 29.25], [20.25, 2.25]),
        ([2.25, 16.25], [29.25, 16.25]),
        ([16.25, 29.25], [16.25, 2.25]),
        ([29.25, 29.25], [2.25, 2.25]),
        ([29.25, 2.25], [2.25, 29.25]),
        ([2.25, 8.25], [48.25, 24.25]),
    ] {
        for shape in 0..4 {
            let depths = shape_depths(shape);
            for behind in [0, 4] {
                let ray = Ray {
                    a,
                    b,
                    weighted: [2.0, 8.0],
                    reciprocal: [1.0, 1.0],
                    skip: 0.0,
                    budget: 256,
                    behind,
                    base: 0,
                    side: 32,
                    levels: 4,
                };
                let expected = oracle(ray, &depths);
                let actual = capture_grid(&context, ray, &depths);
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        (actual - expected).abs() < 0.025,
                        "{ray:?} shape {shape} channel {index}: {actual} vs {expected}"
                    );
                }
                if expected[5] > 0.0 {
                    hits += 1;
                } else {
                    misses += 1;
                }
            }
        }
    }
    assert!(hits > 0 && misses > 0);
}
#[test]
fn gpu_grid_pass_behind_budget_skip_and_perspective_match_cpu() {
    let context = context();
    let depths = pyramid(
        (0..1024)
            .map(|i| if i % 32 == 14 { 1.0 } else { 5.0 })
            .collect(),
    );
    let ray = Ray {
        a: [2.25, 16.25],
        b: [29.25, 16.25],
        weighted: [2.0, 8.0],
        reciprocal: [1.0, 1.0],
        skip: 0.0,
        budget: 256,
        behind: 0,
        base: 0,
        side: 32,
        levels: 4,
    };
    assert!(oracle(ray, &depths)[5] < 0.5);
    assert!(oracle(Ray { behind: 4, ..ray }, &depths)[5] > 0.5);
    for ray in [
        Ray { budget: 1, ..ray },
        Ray { skip: 100.0, ..ray },
        Ray {
            weighted: [1.0, 1.0],
            reciprocal: [0.5, 0.125],
            behind: 4,
            ..ray
        },
    ] {
        let actual = capture_grid(&context, ray, &depths);
        for (actual, expected) in actual.iter().zip(oracle(ray, &depths)) {
            assert!((actual - expected).abs() < 0.025);
        }
    }
}

#[test]
fn gpu_default_trace_bytes_match_pre_grid_shader() {
    let context = context();
    let old = gpu::trace_source(1, false).replace(
        &include_str!("trace.wgsl").replace("SAMPLE_LANES", "1"),
        &include_str!("legacy_trace_test.wgsl").replace("SAMPLE_LANES", "1"),
    );
    assert!(!old.contains("grid_reflection"));
    for scale in [1.0, 10.0] {
        for occluder in [false, true] {
            let current =
                controls_tests::capture_with_shader(&context, settings(), scale, occluder, None);
            let retired = controls_tests::capture_with_shader(
                &context,
                settings(),
                scale,
                occluder,
                Some(&old),
            );
            let case = format!("scale {scale} occluder {occluder}");
            controls_tests::assert_traces_agree(&current, &retired, &case);
            controls_tests::assert_centre_matches_model(&current, !occluder, &case);
            controls_tests::assert_centre_matches_model(&retired, !occluder, &case);
        }
    }
}

#[test]
fn grid_policy_compiles_opaque_water_msaa() {
    let context = context();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut screen = SceneMetadata::new(context.device(), [33, 25], neutral_ao()).unwrap();
        let ssr = Ssr::new(
            context.device(),
            FrameConfig::new([33, 25], wgpu::TextureFormat::Rgba16Float, samples),
            SceneSsr {
                traversal: SceneSsrTraversal::Grid,
                ..settings()
            },
            &mut screen,
        )
        .unwrap();
        assert_eq!(
            ssr.hierarchy
                .iter()
                .map(wgpu::Texture::width)
                .collect::<Vec<_>>(),
            [33, 17, 9, 5]
        );
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
}

#[test]
fn gpu_grid_production_trace_and_average_reduction() {
    let context = context();
    let policy = SceneSsr {
        traversal: SceneSsrTraversal::Grid,
        ..settings()
    };
    let bytes = controls_tests::capture_with_shader(&context, policy, 1.0, false, None);
    assert!(controls_tests::center_red(&bytes) > 0);
    let mut screen = SceneMetadata::new(context.device(), [32, 32], neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new([32, 32], wgpu::TextureFormat::Rgba16Float, 1),
        policy,
        &mut screen,
    )
    .unwrap();
    let expected = pyramid((0..1024).map(|i| f64::from(i % 17 + 1)).collect());
    upload_hierarchy(
        &context,
        &ssr,
        [32, 32],
        expected[0]
            .iter()
            .map(|v| half::f16::from_f64(*v).to_f32())
            .collect::<Vec<_>>(),
    );
    for (texture, expected) in ssr.hierarchy.iter().zip(&expected) {
        let actual = read(&context, texture);
        for (bytes, expected) in actual.chunks_exact(4).zip(expected) {
            assert!(
                (f64::from(f32::from_le_bytes(bytes.try_into().unwrap())) - expected).abs() < 1e-6
            );
        }
    }
}

#[test]
fn gpu_grid_average_partial_bins_match_valid_tap_oracle() {
    let context = context();
    let mut size = [9u32, 5];
    let mut values: Vec<f32> = (1..=45u16).map(f32::from).collect();
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        SceneSsr {
            traversal: SceneSsrTraversal::Grid,
            ..settings()
        },
        &mut screen,
    )
    .unwrap();
    upload_hierarchy(&context, &ssr, size, values.clone());
    for texture in ssr.final_depth.textures() {
        for (bytes, expected) in read(&context, texture).chunks_exact(4).zip(&values) {
            assert!((f32::from_le_bytes(bytes.try_into().unwrap()) - expected).abs() < 1e-6);
        }
        let next_size = size.map(|n| n.div_ceil(2));
        let mut next = vec![];
        for y in 0..next_size[1] {
            for x in 0..next_size[0] {
                let mut sum = 0.0;
                let mut count = 0.0;
                for dy in 0..2 {
                    for dx in 0..2 {
                        if x * 2 + dx < size[0] && y * 2 + dy < size[1] {
                            sum += values
                                [usize::try_from((y * 2 + dy) * size[0] + x * 2 + dx).unwrap()];
                            count += 1.0;
                        }
                    }
                }
                next.push(sum / count);
            }
        }
        values = next;
        size = next_size;
    }
}
