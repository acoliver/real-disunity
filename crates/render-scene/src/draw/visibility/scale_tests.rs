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
