use super::*;
use worldrunner_render_plan::SceneSsrGridDomain;

#[test]
fn gpu_grid_sixteen_texture_boundaries_precede_allocation() {
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(device.limits().max_sampled_textures_per_shader_stage, 16);
    for (domain, count, accepted) in [
        (SceneSsrGridDomain::FullResolution, 8, true),
        (SceneSsrGridDomain::FullResolution, 9, false),
        (SceneSsrGridDomain::ReflectionOutput, 7, true),
        (SceneSsrGridDomain::ReflectionOutput, 8, false),
    ] {
        check_boundary(&device, domain, count, accepted);
    }
    allocation_sentinel(&device);
}

fn check_boundary(device: &wgpu::Device, domain: SceneSsrGridDomain, count: u32, accepted: bool) {
    let size = [129, 65];
    let mut screen = SceneMetadata::new(device, size, neutral_ao()).unwrap();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = Ssr::new(
        device,
        FrameConfig::new(
            size,
            wgpu::TextureFormat::Rgba16Float,
            if accepted { 1 } else { 3 },
        ),
        SceneSsr {
            traversal: SceneSsrTraversal::Grid,
            grid_domain: domain,
            resolution_scale_micro: 500_000,
            grid_level_count: Some(count),
            ..settings()
        },
        &mut screen,
    );
    if accepted {
        assert!(result.is_ok(), "{domain:?} {count}: {result:?}");
    } else {
        assert!(matches!(
            result,
            Err(FrameError::SsrSettings(
                "grid levels exceed device texture binding limit"
            ))
        ));
        assert!(screen.geometry.forward.is_none());
    }
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn allocation_sentinel(device: &wgpu::Device) {
    // A three-sample allocation would fail inside the scope if preflight did not return first.
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _sentinel = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preflight allocation sentinel"),
        size: wgpu::Extent3d {
            width: 129,
            height: 65,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 3,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(pollster::block_on(scope.pop()).is_some());
}

fn odd_pyramid() -> Vec<Vec<f64>> {
    let mut levels = vec![
        (0..33 * 33)
            .map(|i| match i % 33 {
                30 | 31 => 4.0,
                32 => 6.0,
                _ => {
                    if i / 33 == 32 {
                        7.0
                    } else {
                        9.0
                    }
                }
            })
            .collect::<Vec<_>>(),
    ];
    let mut side = 33usize;
    while side > 1 {
        let next_side = side.div_ceil(2);
        let previous = levels.last().unwrap();
        let mut next = Vec::new();
        for y in 0..next_side {
            for x in 0..next_side {
                next.push(odd_average(previous, side, [x, y]));
            }
        }
        levels.push(next);
        side = next_side;
    }
    levels
}

fn odd_average(previous: &[f64], side: usize, cell: [usize; 2]) -> f64 {
    let mut taps = Vec::new();
    for dy in 0..2 {
        for dx in 0..2 {
            if 2 * cell[0] + dx < side && 2 * cell[1] + dy < side {
                taps.push(previous[(2 * cell[1] + dy) * side + 2 * cell[0] + dx]);
            }
        }
    }
    taps.iter().sum::<f64>() / f64::from(u32::try_from(taps.len()).unwrap())
}

#[test]
fn gpu_grid_odd_partial_bins_projection_and_hits_match_cpu() {
    let context = context();
    let depths = odd_pyramid();
    assert!((depths[1][15] - 4.0).abs() < f64::EPSILON);
    assert!((depths[1][16] - 6.0).abs() < f64::EPSILON);
    let mut hits = 0;
    let mut misses = 0;
    let mut edge_hits = 0;
    for (index, (a, b)) in [
        ([2.25, 10.25], [36.25, 10.25]),
        ([32.75, 10.25], [2.25, 10.25]),
        ([2.25, 32.25], [36.25, 32.25]),
        ([32.75, 32.25], [2.25, 32.25]),
        ([31.25, 2.25], [31.25, 36.25]),
        ([31.25, 32.75], [31.25, 2.25]),
        ([32.25, 2.25], [32.25, 36.25]),
        ([32.25, 32.75], [32.25, 2.25]),
        ([31.0, 32.25], [40.0, 32.25]),
    ]
    .into_iter()
    .enumerate()
    {
        let ray = Ray {
            a: a.map(|v| v / 2.0),
            b: b.map(|v| v / 2.0),
            weighted: if index == 8 { [2.0, 20.0] } else { [2.0, 8.0] },
            reciprocal: [1.0, 1.0],
            skip: 0.0,
            budget: 256,
            behind: 4,
            base: 1,
            side: 33,
            levels: 6,
        };
        let expected = oracle(ray, &depths[1..]);
        let actual = capture_grid(&context, ray, &depths[1..]);
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.025,
                "{ray:?}: {actual} != {expected}"
            );
        }
        if index == 8 {
            assert!(expected[5] < 0.5 && expected[4] > 0.0);
        }
        edge_hits += usize::from(expected[5] > 0.5 && (expected[6] > 31.5 || expected[7] > 31.5));
        hits += usize::from(expected[5] > 0.5);
        misses += usize::from(expected[5] < 0.5);
    }
    assert!(hits > 0 && misses > 0 && edge_hits > 0);
}
