use super::SceneShadowCascades;

#[test]
fn camera_splits_are_explicit_and_do_not_reinterpret_coverage_boxes() {
    let extents = vec![1_000, 4_000, 16_000, 64_000];
    let boxes = SceneShadowCascades::new(extents.clone(), 100_000).unwrap();
    let splits = SceneShadowCascades::camera_splits(extents.clone(), 100_000).unwrap();
    assert!(boxes.split_distances_millimeters().is_none());
    assert_eq!(
        splits.split_distances_millimeters(),
        Some(extents.as_slice())
    );
    assert_eq!(boxes.extents_millimeters(), extents);
    assert_eq!(splits.blend_band_micro(), 100_000);
    assert_ne!(boxes, splits);
}

#[test]
fn explicit_split_policy_keeps_ordering_and_blend_validation() {
    for distances in [vec![], vec![0], vec![4_000, 4_000], vec![4_000, 1_000]] {
        assert!(SceneShadowCascades::camera_splits(distances, 0).is_err());
    }
    assert!(SceneShadowCascades::camera_splits(vec![4_000], 1_000_001).is_err());
    SceneShadowCascades::camera_splits(vec![4_000], 1_000_000).unwrap();
}
