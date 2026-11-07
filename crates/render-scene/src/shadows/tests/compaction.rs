//! Cascade passes draw one instanced draw per (batch, LOD) group, however fragmented the selection.
use super::*;

#[test]
fn gpu_cascade_draw_counts_follow_groups_not_ranges() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = super::cascades::source_with(
        &scene(
            &[
                [0, 0, 1_000],
                [0, 1_100, 1_000],
                [0, 1_700, 1_000],
                [0, 2_600, 1_000],
            ],
            600_000,
            None,
        ),
        SceneShadowCascades::new(vec![650, 1_200, 2_000, 3_500], 100_000).unwrap(),
    );
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    // Hiding the second caster leaves a hole in the caster batch's slot range.
    let hidden = renderer.scene_mut().instance_handles().nth(2).unwrap();
    renderer
        .scene_mut()
        .set_instance_visible(hidden, false)
        .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let counts = renderer.scene_mut().statistics().passes.shadow_cascades;
    let mut split = false;
    for (index, count) in counts.iter().enumerate() {
        let (ranges, groups) = renderer.scene_mut().published_ranges_and_groups(index + 1);
        assert_eq!(
            count.calls,
            u64::try_from(groups).unwrap(),
            "cascade {index}"
        );
        split |= ranges > groups;
    }
    assert!(split, "no cascade selected a fragmented batch");
}
