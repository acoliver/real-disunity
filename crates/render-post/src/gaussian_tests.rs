use super::*;

#[test]
fn folded_weights_preserve_normalization_and_discrete_convolution() {
    for radius in 0..=MAX_GAUSSIAN_RADIUS {
        let folded = gaussian_weights(radius).unwrap();
        let sum = folded.center + 2.0 * folded.taps.iter().map(|tap| tap.weight).sum::<f32>();
        assert!((sum - 1.0).abs() < 1e-6, "radius {radius}: {sum}");
        let discrete = discrete_weights(radius).unwrap();
        // A nonlinear signal reveals incorrect fractional offsets and pairing.
        let signal = |x: f32| (x * 0.17).sin().powi(2);
        let sample = |x: f32| {
            let low = x.floor();
            signal(low) * (1.0 - (x - low)) + signal(low + 1.0) * (x - low)
        };
        for center in [-11.0, 0.0, 7.0] {
            let expected = discrete[0] * signal(center)
                + discrete
                    .iter()
                    .enumerate()
                    .skip(1)
                    .map(|(i, w)| {
                        let offset = f32::from(u16::try_from(i).unwrap());
                        w * (signal(center - offset) + signal(center + offset))
                    })
                    .sum::<f32>();
            let actual = folded.center * signal(center)
                + folded
                    .taps
                    .iter()
                    .map(|tap| {
                        tap.weight * (sample(center - tap.offset) + sample(center + tap.offset))
                    })
                    .sum::<f32>();
            assert!(
                (actual - expected).abs() < 1e-6,
                "radius {radius}: {actual} != {expected}"
            );
        }
    }
    assert!(gaussian_weights(MAX_GAUSSIAN_RADIUS + 1).is_err());
}

#[test]
fn odd_extents_reach_one_exactly_and_excess_levels_fail() {
    assert_eq!(
        gaussian_extents([37, 23], 6).unwrap(),
        [[19, 12], [10, 6], [5, 3], [3, 2], [2, 1], [1, 1]]
    );
    assert!(gaussian_extents([37, 23], 7).is_err());
    assert_eq!(gaussian_extents([1, 1], 1).unwrap(), [[1, 1]]);
    assert!(gaussian_extents([1, 1], 2).is_err());
    assert!(gaussian_extents([0, 4], 1).is_err());
}

#[test]
fn gaussian_policy_rejects_nonfinite_and_out_of_range_levels() {
    let valid = GaussianLevel {
        radius: 44,
        tint: [1.0; 3],
        weight: 0.5,
    };
    let check = |level| BloomKernel::SeparablePyramid([level; MAX_BLOOM_LEVELS]).validate(1);
    assert!(check(valid).is_ok());
    for radius in [65, u32::MAX] {
        assert!(check(GaussianLevel { radius, ..valid }).is_err());
    }
    for weight in [f32::NAN, f32::INFINITY, -1.0, 65.0] {
        assert!(check(GaussianLevel { weight, ..valid }).is_err());
        assert!(
            check(GaussianLevel {
                tint: [weight, 1.0, 1.0],
                ..valid
            })
            .is_err()
        );
    }
}

#[test]
fn axis_kernel_support_sigma_and_clamping_follow_authored_rule() {
    for (extent, support, sigma) in [(1, 2, 2.5 / 3.0), (3, 4, 4.5 / 3.0), (80, 44, 44.5 / 3.0)] {
        let rule = crate::GaussianKernelRule::AxisClampedPairs;
        let (actual_support, actual_sigma) = rule.parameters(44, extent).unwrap();
        assert_eq!(actual_support, support);
        assert!((actual_sigma - sigma).abs() < 1e-6);
        let weights = rule.weights(44, extent).unwrap();
        assert_eq!(weights.taps.len(), usize::try_from(support / 2).unwrap());
        let sum = weights.center + 2.0 * weights.taps.iter().map(|tap| tap.weight).sum::<f32>();
        assert!((sum - 1.0).abs() < 1e-6);
        let ratio = (-0.5 / (sigma * sigma)).exp();
        let discrete_sum = 1.0
            + 2.0
                * (1..=support)
                    .map(|i| {
                        let x = f32::from(u16::try_from(i).unwrap()) / sigma;
                        (-0.5 * x * x).exp()
                    })
                    .sum::<f32>();
        assert!((weights.center - 1.0 / discrete_sum).abs() < 1e-6);
        assert!(
            (weights.taps[0].weight - (ratio + (-2.0 / (sigma * sigma)).exp()) / discrete_sum)
                .abs()
                < 1e-6
        );
    }
    let rule = crate::GaussianKernelRule::AxisClampedPairs;
    assert_eq!(rule.parameters(0, 0).unwrap(), (0, 0.0));
    assert_eq!(rule.parameters(3, 0).unwrap().0, 2);
    assert!(rule.parameters(65, 100).is_err());
}
