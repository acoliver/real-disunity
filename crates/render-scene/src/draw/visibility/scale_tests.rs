use super::tests::{level, mesh, scene, transform};
use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneObject};

#[test]
fn wide_scaled_ceilings_match_unsquared_oracle() {
    for ceiling in [3_u64, 1_000, 9_007_199_254_740_993, u64::MAX / 100] {
        for scale in [1_u32, 500_000, 1_000_000, 2_000_000, 100_000_000] {
            let boundary = u128::from(ceiling) * u128::from(scale) / 1_000_000;
            for distance in [boundary.saturating_sub(1), boundary, boundary + 1] {
                let expected = distance * 1_000_000 <= u128::from(ceiling) * u128::from(scale);
                assert_eq!(
                    within_ceiling(distance.pow(2), u128::from(ceiling).pow(2), Some(scale)),
                    expected
                );
            }
        }
    }
}

#[test]
fn scaled_selection_matches_independent_integer_oracle() {
    let selection = MeshSelection::new(&mesh());
    for scale in [500_000_u32, 1_000_000, 2_000_000] {
        for ceiling in [1_000_u64, 2_000, 4_000] {
            let boundary = ceiling * u64::from(scale) / 1_000_000;
            for distance in [boundary - 1, boundary, boundary + 1] {
                let expected = [1_000_u64, 2_000, 4_000]
                    .iter()
                    .position(|&c| distance * 1_000_000 <= c * u64::from(scale))
                    .unwrap_or(2);
                let squared = u128::from(distance).pow(2);
                assert_eq!(selection.lod(squared, Some(scale)), expected);
                assert_eq!(
                    selection.lod(squared, None),
                    selection.lod(squared, Some(1_000_000))
                );
            }
        }
    }
}

#[test]
fn rational_boundaries_and_full_width_inputs_do_not_round_or_overflow() {
    let selection = MeshSelection::new(
        &SceneMesh::with_distances(vec![level(0), level(1)], vec![3, u64::MAX]).unwrap(),
    );
    for (squared, expected) in [(2, 0), (3, 1)] {
        assert_eq!(selection.lod(squared, Some(500_000)), expected);
    }
    let large = u128::from(u64::MAX).pow(2);
    for scale in [1, 500_000, 1_000_000, 2_000_000, 100_000_000] {
        assert_eq!(
            within_ceiling(large, large, Some(scale)),
            scale >= 1_000_000
        );
    }
    assert!(within_ceiling(u128::MAX, large, Some(100_000_000)));
    assert!(!within_ceiling(u128::MAX, large, Some(1)));
}

#[test]
fn retained_scale_and_transform_updates_rebuild_camera_and_shadow_selection() {
    let mut instances = Instances::new(&scene());
    let object = SceneObject::new(
        transform([0, 0, 500]),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let handle = instances.add(object, true).unwrap();
    let meshes = [MeshSelection::new(&mesh())];
    for (scale, expected) in [(500_000, 2), (1_000_000, 1), (2_000_000, 0)] {
        let object = object.with_lod_distance_scale_millionths(scale).unwrap();
        instances.set_object(handle, object).unwrap();
        instances
            .set_transform(handle, *object.transform())
            .unwrap();
        assert_eq!(
            instances
                .get(handle)
                .unwrap()
                .0
                .lod_distance_scale_millionths(),
            Some(scale)
        );
        for matrix in [
            Mat4::IDENTITY,
            caster_matrix(&instances, &meshes, None, Mat4::IDENTITY),
        ] {
            let mut selection = Selection::default();
            selection.rebuild(&instances, &meshes, matrix, [0, 0, -1_000]);
            assert_eq!(selection.stats.culled, 0);
            assert_eq!(selection.ranges[0].lod, expected);
        }
    }
}

#[test]
fn scale_validation_and_default_instance_bytes() {
    let object = SceneObject::new(
        transform([0, 0, 500]),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    assert_eq!(object.lod_distance_scale_millionths(), None);
    assert_eq!(
        crate::geometry::pack_object(object),
        crate::geometry::pack_instance(object.transform())
    );
    for scale in [0, 100_000_001, u32::MAX] {
        assert!(object.with_lod_distance_scale_millionths(scale).is_err());
    }
    for scale in [1, 100_000_000] {
        assert!(object.with_lod_distance_scale_millionths(scale).is_ok());
    }
}

#[test]
fn scaled_ceilings_admit_exactly_the_distances_the_rational_test_admits() {
    let mut value = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        value
    };
    let mut ceilings: Vec<u128> = [0_u64, 1, 3, 999_999_999_999, 1_000_000_000_000, 4_000_000]
        .map(u128::from)
        .into_iter()
        .chain([u128::from(u64::MAX).pow(2), u128::MAX, u128::MAX - 1])
        .collect();
    let mut scales = vec![
        None,
        Some(1),
        Some(7),
        Some(999_999),
        Some(1_000_000),
        Some(100_000_000),
    ];
    for _ in 0..2_000 {
        let meters = u128::from(next() % 2_000_000);
        ceilings.push(meters * meters + u128::from(next() % 1_000));
        scales.push(Some(1 + u32::try_from(next() % 100_000_000).unwrap()));
    }
    for (index, &ceiling) in ceilings.iter().enumerate() {
        for &scale in scales
            .iter()
            .skip(index % 7)
            .step_by(97)
            .chain(&scales[..6])
        {
            let limit = scaled_ceiling(ceiling, scale);
            let mut distances = vec![0, 1, u128::MAX, ceiling, u128::from(next())];
            distances.extend([limit.saturating_sub(1), limit, limit.saturating_add(1)]);
            for distance in distances {
                assert_eq!(
                    distance <= limit,
                    within_ceiling(distance, ceiling, scale),
                    "{distance} {ceiling} {scale:?}"
                );
            }
        }
    }
}

/// The squared distance as it was computed in 128-bit signed arithmetic throughout.
fn distance_squared_reference(position: [i64; 3], eye: [i64; 3]) -> Option<u128> {
    position
        .into_iter()
        .zip(eye)
        .try_fold(0_u128, |sum, (position, eye)| {
            let delta = (i128::from(position) - i128::from(eye)).unsigned_abs();
            sum.checked_add(delta * delta)
        })
}

#[test]
fn squared_distances_equal_the_wide_computation_around_the_narrow_limit() {
    let limit = 1_i64 << 31;
    let values = [
        0,
        1,
        -1,
        999,
        limit - 1,
        limit,
        limit + 1,
        -limit + 1,
        -limit,
        -limit - 1,
        3 * limit,
        i64::from(i32::MAX),
        i64::from(i32::MIN),
        i64::MAX / 3,
        i64::MIN / 3,
        i64::MAX,
        i64::MIN,
    ];
    let mut checked = 0;
    for &x in &values {
        for &y in &values {
            for &z in &values {
                for eye in [
                    [0, 0, 0],
                    [x, y, z],
                    [y, z.saturating_neg(), 5],
                    [limit, -limit, 7],
                ] {
                    // Only the sums of squares of 2^64-wide differences overflow.
                    if let Some(expected) = distance_squared_reference([x, y, z], eye) {
                        assert_eq!(distance_squared([x, y, z], eye), expected);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 10_000);
}

#[test]
#[should_panic(expected = "squared placement distance exceeds u128 millimetres")]
fn a_squared_distance_beyond_u128_still_panics() {
    let _ = distance_squared(
        [i64::MIN, i64::MIN, i64::MIN],
        [i64::MAX, i64::MAX, i64::MAX],
    );
}

/// Ceilings, squared, of the shapes a mesh can have: ascending, repeating, out of order,
/// a lone level and a ceiling no distance exceeds.
fn ceiling_sets() -> Vec<Vec<u128>> {
    let squared = |distances: &[u128]| distances.iter().map(|d| d * d).collect();
    vec![
        squared(&[40, 100, 230]),
        squared(&[40, 40, 230]),
        squared(&[100, 40, 230]),
        squared(&[0, 7, 8]),
        squared(&[90]),
        vec![],
        vec![u128::MAX, u128::MAX],
        vec![1_999, 2_000, 2_001, 90_000],
    ]
}

#[test]
fn a_margin_holds_the_level_for_every_eye_within_it_and_wastes_little() {
    for ceilings in ceiling_sets() {
        for distance in 0_i64..320 {
            let squared = u128::try_from(distance * distance).unwrap();
            let level = MeshSelection::level(squared, &ceilings);
            let margin = MeshSelection::level_margin(squared, &ceilings, level);
            // Eyes on the line through the placement are the farthest a move can change the
            // distance, so the margin is the nearest of them to choose another level.
            let nearest_other = (1_i64..400)
                .find(|&moved| {
                    [distance - moved, distance + moved].into_iter().any(|to| {
                        let to = u128::try_from(to * to).unwrap();
                        MeshSelection::level(to, &ceilings) != level
                    })
                })
                .map(|moved| u64::try_from(moved - 1).unwrap());
            match nearest_other {
                Some(reach) => {
                    assert!(margin <= reach, "{ceilings:?} at {distance}: {margin}");
                    assert!(margin + 2 >= reach, "{ceilings:?} at {distance}: {margin}");
                }
                None => assert!(margin >= 397, "{ceilings:?} at {distance}: {margin}"),
            }
        }
    }
}

#[test]
fn a_margin_holds_the_level_for_eyes_in_every_direction_and_for_wide_distances() {
    let mut state = 7_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let span = |value: u64, bound: u64| i64::try_from(value % bound).unwrap();
    for ceilings in ceiling_sets() {
        for _ in 0..4_000 {
            let position = [0, 1, 2].map(|_| span(next(), 600) - 300);
            let eye = [0, 1, 2].map(|_| span(next(), 600) - 300);
            let squared = distance_squared(position, eye);
            let level = MeshSelection::level(squared, &ceilings);
            let margin = MeshSelection::level_margin(squared, &ceilings, level).min(300);
            for _ in 0..8 {
                // A move of at most `margin` millimetres whole.
                let step = [0, 1, 2]
                    .map(|_| span(next(), 2 * margin + 1) - i64::try_from(margin).unwrap());
                let length = step
                    .iter()
                    .map(|d| u128::try_from(d * d).unwrap())
                    .sum::<u128>();
                if length > u128::from(margin) * u128::from(margin) {
                    continue;
                }
                let moved = [0, 1, 2].map(|axis| eye[axis] + step[axis]);
                assert_eq!(
                    MeshSelection::level(distance_squared(position, moved), &ceilings),
                    level,
                    "{ceilings:?}: {position:?} from {eye:?} to {moved:?}"
                );
            }
        }
    }
    // Distances beyond what f64 holds exactly keep the margin exact to the rounded roots.
    let far = u128::from(u64::MAX);
    let ceilings = [far * far, u128::MAX];
    let squared = (far - 1_000).pow(2);
    assert_eq!(MeshSelection::level(squared, &ceilings), 0);
    let margin = MeshSelection::level_margin(squared, &ceilings, 0);
    assert_eq!(margin, 999);
    assert_eq!(
        MeshSelection::level((far - 1_000 + u128::from(margin)).pow(2), &ceilings),
        0
    );
}
