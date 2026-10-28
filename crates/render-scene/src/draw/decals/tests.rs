use super::*;
use crate::draw::tests::decals::{decal, source};

#[test]
fn two_thousand_authored_placements_are_culled_before_the_frame_bound() {
    let placements: Vec<_> = (0..2000)
        .map(|i| {
            let mut d = decal();
            d.transform.position_millimeters[0] = if i < 3 { 0 } else { 1_000_000 + i * 1000 };
            d
        })
        .collect();
    let scene = source(false, false).with_decals(placements).unwrap();
    let entries: Vec<_> = scene
        .decals()
        .iter()
        .enumerate()
        .map(|(i, &d)| entry(&scene, i as u64, d))
        .collect();
    assert_eq!(scene.decals().len(), 2000);
    assert_eq!(
        selected(&entries, Mat4::IDENTITY, Vec3::ZERO),
        vec![0, 1, 2]
    );
    let matrix = Mat4::from_translation(Vec3::new(-1003.0, 0.0, 0.0));
    assert!(!selected(&entries, matrix, Vec3::new(1003.0, 0.0, 0.0)).is_empty());
}

#[test]
fn overflow_uses_priority_distance_and_stable_id_but_preserves_compositing_order() {
    let scene = source(false, false);
    let mut entries: Vec<_> = (0..40).map(|i| entry(&scene, 100 - i, decal())).collect();
    entries[0].decal.priority = 1;
    entries[1].model.w_axis.x = 0.1;
    let chosen = selected(&entries, Mat4::IDENTITY, Vec3::ZERO);
    let mut expected = vec![0];
    expected.extend(9..40);
    assert_eq!(chosen, expected);
    assert_eq!(chosen, selected(&entries, Mat4::IDENTITY, Vec3::ZERO));
    let moved = selected(&entries, Mat4::IDENTITY, Vec3::new(0.1, 0.0, 0.0));
    assert!(moved.contains(&1));
    assert_eq!(moved.len(), LIMIT);
}
