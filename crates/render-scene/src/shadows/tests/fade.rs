use super::cascades::source_with;
use super::*;

const SHADOWED: Vec3 = Vec3::new(-0.93, 0.0, 0.0);

/// Brightness of the shadowed receiver for each fade fraction.
fn shadowed_with_fades(policy: &SceneShadowCascades, fades: &[u64]) -> Vec<u8> {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, Some([255, 255, 0, 0]));
    fades
        .iter()
        .map(|&fade| {
            let with_fade = source_with(&source, policy.clone().with_fade_out(fade).unwrap());
            let image = capture_scene_shadowed(&context, &with_fade, &view(), settings()).unwrap();
            at(&image, SHADOWED)
        })
        .collect()
}

#[test]
fn shadow_fade_out_rejects_fractions_above_one() {
    let cascades = SceneShadowCascades::camera_splits(vec![8_000], 0).unwrap();
    assert_eq!(cascades.fade_out_micro(), 0);
    assert!(cascades.clone().with_fade_out(1_000_001).is_err());
    assert_eq!(
        cascades.with_fade_out(300_000).unwrap().fade_out_micro(),
        300_000
    );
}

#[test]
fn gpu_camera_split_shadows_fade_to_lit_across_the_far_band() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let policy = SceneShadowCascades::camera_splits(vec![8_000], 0).unwrap();
    let source = source_with(
        &scene(&[[0, 0, 1_000]], 600_000, Some([255, 255, 0, 0])),
        policy.clone(),
    );
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let unfaded = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    assert_shadow(&unfaded, &baseline, SHADOWED);
    // The receiver lies at about 95% of the 8 m shadow distance.
    let fades = [0, 20_000, 100_000, 500_000, 1_000_000];
    let values = shadowed_with_fades(&policy, &fades);
    assert_eq!(
        values[0],
        at(&unfaded, SHADOWED),
        "zero fade is the hard end"
    );
    assert_eq!(
        values[1], values[0],
        "a band beyond the receiver leaves it shadowed"
    );
    for pair in values[1..].windows(2) {
        assert!(
            pair[1] > pair[0],
            "wider bands lighten monotonically: {values:?}"
        );
    }
    let lit = at(&baseline, SHADOWED);
    assert!(
        u16::from(lit - values[4]) * 5 < u16::from(lit - values[0]),
        "the full band leaves less than a fifth of the shadow: {values:?}, lit {lit}"
    );
    // Lit receivers stay lit for every band.
    let half_band = source_with(&source, policy.with_fade_out(500_000).unwrap());
    let image = capture_scene_shadowed(&context, &half_band, &view(), settings()).unwrap();
    assert_lit(&image, &baseline, Vec3::new(-0.57, 0.0, 0.0));
}

#[test]
fn gpu_coverage_box_shadows_fade_with_the_last_box_extent() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let policy = SceneShadowCascades::new(vec![2_000], 0).unwrap();
    let values = shadowed_with_fades(&policy, &[0, 500_000, 1_000_000]);
    assert!(
        values[0] <= values[1] && values[1] <= values[2],
        "{values:?}"
    );
    assert!(
        values[2] > values[0],
        "a full band reaches the receiver: {values:?}"
    );
}
