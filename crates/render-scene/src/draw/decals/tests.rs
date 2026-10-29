use super::*;
use crate::draw::tests::decals::{decal, source};

#[test]
fn normal_frame_packing_preserves_default_bytes_and_texture_layer_bits() {
    use worldrunner_render_plan::{SceneDecalMapping, SceneDecalNormalFrame};
    let d = decal();
    let mut values = Vec::new();
    mapping_values(&mut values, &d, 1.0);
    let actual: Vec<_> = values.into_iter().flat_map(f32::to_le_bytes).collect();
    let expected: Vec<_> = [1.0_f32, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        normal_frame_bits(SceneDecalMapping::default().normal_frame),
        0
    );
    for frame in [
        SceneDecalNormalFrame::Projection,
        SceneDecalNormalFrame::ReceiverWorld,
    ] {
        let flags = normal_frame_bits(frame);
        assert_eq!(
            flags >> 19,
            u32::from(frame == SceneDecalNormalFrame::ReceiverWorld)
        );
        for mask in 0..8 {
            let packed = flags | 0x11 | (32 << 8) | ((7 ^ mask) << 16);
            assert_eq!(packed & 255, 17);
            assert_eq!((packed >> 8) & 255, 32);
            assert_eq!((packed >> 16) & 7, 7 ^ mask);
            assert_eq!(packed >> 19, flags >> 19);
        }
    }
}

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

#[test]
fn alpha_test_selection_packing_preserves_default_receiver_bytes_and_boundaries() {
    let mut d = decal();
    assert_eq!(receiver_and_alpha_test(&d), d.receivers.bits());
    for cutoff in [0, 501_961, 1_000_000] {
        d.alpha_test_cutoff_micro = Some(cutoff);
        let packed = receiver_and_alpha_test(&d);
        assert_eq!(packed & 3, d.receivers.bits());
        assert_ne!(packed & (1 << 22), 0);
        assert_eq!((packed >> 2) & 0xfffff, cutoff);
    }
}
