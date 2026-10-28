use super::*;

#[test]
fn active_cap_is_applied_after_outward_near_quantization() {
    let fit = RangeFit {
        active_maximum: Some(1000.5),
        ..policy()
    };
    assert_eq!(
        fit.fit(
            DepthRange::Receivers {
                near: 1000.75,
                far: 1050.0
            },
            [0.1, 2000.0]
        )
        .unwrap(),
        DepthRange::Receivers {
            near: 1000.0,
            far: 1000.5
        }
    );
}

#[test]
fn default_fitting_matches_previous_equations_bit_for_bit() {
    let fit = RangeFit {
        near_step: 0.2,
        far_step: 10.0,
        minimum_span_policy: MinimumSpanPolicy::ExtendNear,
        ..policy()
    };
    for (observed_near, observed_far) in [
        (0.15, 0.2),
        (35.3, 105.5),
        (1000.0, 1050.0),
        (1900.5, 1999.0),
    ] {
        let far = ((observed_far / fit.far_step).ceil() * fit.far_step)
            .max(0.1 + fit.minimum_span)
            .min(2000.0);
        let near = ((observed_near / fit.near_step).floor() * fit.near_step)
            .max(0.1)
            .min(far - fit.minimum_span.min(far - 0.1));
        let DepthRange::Receivers {
            near: actual_near,
            far: actual_far,
        } = fit
            .fit(
                DepthRange::Receivers {
                    near: observed_near,
                    far: observed_far,
                },
                [0.1, 2000.0],
            )
            .unwrap()
        else {
            panic!("receivers expected")
        };
        assert_eq!(
            [actual_near.to_bits(), actual_far.to_bits()],
            [near.to_bits(), far.to_bits()]
        );
    }
}

fn policy() -> RangeFit {
    RangeFit {
        near_step: 1.0,
        far_step: 1.0,
        minimum_span: 200.0,
        minimum_span_policy: MinimumSpanPolicy::ExtendFar,
        active_maximum: None,
        split_lambda: 0.5,
        split_policy: crate::SplitPolicy::Practical,
    }
}

#[test]
fn minimum_request_preserves_near_and_caps_far_after_extension() {
    let depth = DepthRange::Receivers {
        near: 1000.0,
        far: 1050.0,
    };
    assert_eq!(
        policy().fit(depth, [0.1, 2000.0]).unwrap(),
        DepthRange::Receivers {
            near: 1000.0,
            far: 1200.0
        }
    );
    for (maximum, expected_far) in [
        (1500.0, 1200.0),
        (1100.0, 1100.0),
        (1025.0, 1025.0),
        (1000.0, 0.0),
        (900.0, 0.0),
    ] {
        let expected = if expected_far == 0.0 {
            DepthRange::EmptyDepth
        } else {
            DepthRange::Receivers {
                near: 1000.0,
                far: expected_far,
            }
        };
        let fit = RangeFit {
            active_maximum: Some(maximum),
            ..policy()
        };
        assert_eq!(fit.fit(depth, [0.1, 2000.0]).unwrap(), expected);
        assert_eq!(
            fit.clip(
                DepthRange::Receivers {
                    near: 1000.0,
                    far: 1200.0
                },
                [0.1, 2000.0]
            )
            .unwrap(),
            expected
        );
    }
    assert_eq!(
        policy().fit(depth, [0.1, 1100.0]).unwrap(),
        DepthRange::Receivers {
            near: 1000.0,
            far: 1100.0
        }
    );
}

#[test]
fn default_minimum_span_retains_near_extension() {
    let depth = DepthRange::Receivers {
        near: 1000.0,
        far: 1050.0,
    };
    let old = RangeFit {
        minimum_span_policy: MinimumSpanPolicy::ExtendNear,
        ..policy()
    };
    assert_eq!(
        old.fit(depth, [0.1, 2000.0]).unwrap(),
        DepthRange::Receivers {
            near: 850.0,
            far: 1050.0
        }
    );
}

#[test]
fn active_maximum_rejects_nonpositive_and_nonfinite_values() {
    for maximum in [0.0, -1.0, f32::INFINITY, f32::NAN] {
        assert!(
            RangeFit {
                active_maximum: Some(maximum),
                ..policy()
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn empty_depth_is_distinct_from_invalid_and_ranges_are_metres() {
    assert_eq!(
        DepthRange::decode(&[f32::MAX.to_bits(), 0, 0, 0]),
        Ok(DepthRange::EmptyDepth)
    );
    assert!(DepthRange::decode(&[f32::MAX.to_bits(), 1, 0, 0]).is_err());
    assert!(DepthRange::decode(&[2.0_f32.to_bits(), 1.0_f32.to_bits(), 2, 0]).is_err());
    assert!(DepthRange::decode(&[1, 2, 1, 1]).is_err());
    assert_eq!(
        DepthRange::decode(&[2.5_f32.to_bits(), 72.0_f32.to_bits(), 8, 0]),
        Ok(DepthRange::Receivers {
            near: 2.5,
            far: 72.0
        })
    );
}

#[test]
fn range_and_splits_round_outward_without_escaping_camera() {
    let fit = RangeFit {
        near_step: 0.5,
        far_step: 10.0,
        minimum_span: 2.0,
        minimum_span_policy: MinimumSpanPolicy::ExtendNear,
        active_maximum: None,
        split_lambda: 0.0,
        split_policy: crate::SplitPolicy::Practical,
    };
    let range = fit
        .fit(
            DepthRange::Receivers {
                near: 2.7,
                far: 71.0,
            },
            [0.1, 100.0],
        )
        .unwrap();
    assert_eq!(
        range,
        DepthRange::Receivers {
            near: 2.5,
            far: 80.0
        }
    );
    assert_eq!(fit.splits(range, 2).unwrap(), vec![41.25, 80.0]);
    assert_eq!(
        fit.fit(DepthRange::EmptyDepth, [0.1, 100.0]).unwrap(),
        DepthRange::EmptyDepth
    );
    assert!(
        fit.fit(
            DepthRange::Receivers {
                near: f32::NAN,
                far: 10.0
            },
            [0.1, 100.0]
        )
        .is_err()
    );
}

#[test]
fn filter_expands_immediately_and_delays_only_contraction() {
    let mut filter = RangeFilter::default();
    let initial = DepthRange::Receivers {
        near: 2.0,
        far: 80.0,
    };
    let contracted = DepthRange::Receivers {
        near: 4.0,
        far: 60.0,
    };
    assert_eq!(filter.update(initial, 10, 2).unwrap(), initial);
    assert_eq!(filter.update(contracted, 11, 2).unwrap(), initial);
    assert_eq!(filter.update(contracted, 12, 2).unwrap(), contracted);
    let expanded = DepthRange::Receivers {
        near: 1.0,
        far: 90.0,
    };
    assert_eq!(filter.update(expanded, 13, 2).unwrap(), expanded);
    assert_eq!(
        filter.update(DepthRange::EmptyDepth, 14, 2).unwrap(),
        DepthRange::EmptyDepth
    );
}

#[test]
fn carried_fit_only_intersects_changed_camera_clip() {
    let depth = DepthRange::Receivers {
        near: 8.5,
        far: 10.0,
    };
    assert_eq!(policy().clip(depth, [0.5, 20.0]).unwrap(), depth);
    assert_eq!(
        policy().clip(depth, [9.0, 9.25]).unwrap(),
        DepthRange::Receivers {
            near: 9.0,
            far: 9.25
        }
    );
    assert_eq!(
        policy().clip(depth, [11.0, 20.0]).unwrap(),
        DepthRange::EmptyDepth
    );
    assert!(policy().clip(depth, [0.0, 20.0]).is_err());
}
