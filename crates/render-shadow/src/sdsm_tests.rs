use super::*;

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
