use super::*;

fn child_ray() -> Ray {
    Ray {
        a: [1.125, 8.125],
        b: [14.625, 8.125],
        weighted: [5.125; 2],
        reciprocal: [1.0; 2],
        skip: 0.0,
        budget: 256,
        behind: 0,
        base: 1,
        side: 32,
        levels: 5,
    }
}

pub(super) fn child_capture(context: &GpuContext, ray: Ray, depths: &[Vec<f64>]) -> Vec<f64> {
    let mut bound = depths[1..=usize::try_from(ray.levels).unwrap()].to_vec();
    let shader = grid_shader(ray);
    if shader
        .matches(&format!("grid_depth_{}", ray.levels))
        .count()
        > 1
    {
        bound.push(depths[0].clone());
    }
    capture_bound_grid(context, ray, &bound)
}

fn assert_child(actual: &[f64], pixel: [f64; 2]) {
    assert_eq!(actual[5].to_bits(), 1.0f64.to_bits(), "miss: {actual:?}");
    assert_eq!(&actual[6..8], &pixel, "wrong child: {actual:?}");
    assert_eq!(actual[2].to_bits(), 5.0f64.to_bits());
}

#[test]
fn gpu_grid_child_foreground_before_background_midpoint() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut full = vec![9000.0; 1024];
    full[16 * 32 + 14] = 5.0;
    let depths = pyramid(full);
    assert_child(&child_capture(&context, child_ray(), &depths), [14.0, 16.0]);
}

#[test]
fn gpu_grid_child_off_ray_minimum_continues_to_real_contact() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut full = vec![9000.0; 1024];
    full[17 * 32 + 14] = 5.0;
    full[16 * 32 + 22] = 5.0;
    let depths = pyramid(full);
    assert_child(&child_capture(&context, child_ray(), &depths), [22.0, 16.0]);
}

#[test]
fn gpu_grid_child_empty_contact_leaf_does_not_end_walk() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut full = vec![9000.0; 1024];
    full[17 * 32 + 14] = 4.0;
    full[16 * 32 + 22] = 5.0;
    let depths = pyramid(full);
    // An off-ray bound behind the ray must not exhaust the zero pass allowance.
    assert_child(&child_capture(&context, child_ray(), &depths), [22.0, 16.0]);
}

#[test]
fn gpu_grid_child_seeded_depths_agree_with_level_zero() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut seed = 0x1406_40fcu32;
    let full = (0..1024)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if seed >> 28 < 3 { 5.0 } else { 9000.0 }
        })
        .collect();
    let depths = pyramid(full);
    for (a, b) in [
        ([2.25, 16.25], [29.25, 16.25]),
        ([29.25, 16.25], [2.25, 16.25]),
        ([16.25, 2.25], [16.25, 29.25]),
        ([16.25, 29.25], [16.25, 2.25]),
    ] {
        let full_ray = Ray {
            a,
            b,
            base: 0,
            levels: 1,
            ..child_ray()
        };
        let expected = oracle(full_ray, &depths[..1]);
        let ray = Ray {
            a: a.map(|v| v / 2.0),
            b: b.map(|v| v / 2.0),
            ..child_ray()
        };
        let actual = child_capture(&context, ray, &depths);
        for channel in [2, 5, 6, 7] {
            assert!(
                (actual[channel] - expected[channel]).abs() < 0.025,
                "{ray:?} channel {channel}: {actual:?} != {expected:?}"
            );
        }
    }
}

#[test]
fn gpu_grid_child_near_corner_intervals_are_not_skipped() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut full = vec![9000.0; 1024];
    full[7 * 32 + 7] = 5.0;
    let depths = pyramid(full);
    for (a, b) in [
        ([1.125, 2.812_011_718_75], [14.625, 9.562_011_718_75]),
        ([14.625, 9.562_011_718_75], [1.125, 2.812_011_718_75]),
        ([2.812_011_718_75, 1.125], [9.562_011_718_75, 14.625]),
        ([9.562_011_718_75, 14.625], [2.812_011_718_75, 1.125]),
    ] {
        let ray = Ray {
            a,
            b,
            ..child_ray()
        };
        assert_child(&child_capture(&context, ray, &depths), [7.0, 7.0]);
    }
}
