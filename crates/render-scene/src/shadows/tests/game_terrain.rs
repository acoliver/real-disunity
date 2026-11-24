//! A game feature on the shadowed route reads the frame's streamed terrain like on the
//! unshadowed one.
use super::*;
use crate::offscreen::OffscreenTarget;

#[test]
fn shadowed_frames_lend_their_streamed_terrain_to_game_features() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = super::streaming::multilevel_source();
    let mut renderer = shadowed(&context, &source, settings()).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let lent = crate::compositor::terrain_probe::install(&mut scheduler, &context);
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for _ in 0..2 {
        render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    }
    let lent = lent.answers();
    assert_eq!(lent.len(), 2, "the probe prepared in both frames");
    assert!(lent[0].is_ok_and(|count| count.is_some_and(|count| count > 0)));
    assert!(
        lent[1].is_ok_and(|count| count.is_some_and(|count| count > 1)),
        "the second frame lends the refined selection: {lent:?}"
    );
}

/// A shadowed renderer fitting its cascades from a same-frame capture of the streamed terrain.
fn same_frame_capture_renderer(context: &GpuContext) -> ShadowedScene {
    use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
    let source = super::streaming::multilevel_source();
    let mut renderer = shadowed(context, &source, settings()).unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: worldrunner_render_shadow::RangeFit {
                    near_step: 0.1,
                    far_step: 1.0,
                    minimum_span: 1.0,
                    minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
                    active_maximum: None,
                    split_lambda: 0.5,
                    split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
                },
            }),
        )
        .unwrap();
    renderer
}

fn render_one_capture(
    renderer: &mut ShadowedScene,
    scheduler: &mut crate::compositor::FrameRenderer,
    context: &GpuContext,
) {
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    renderer
        .render_depth_fitted_host(
            context,
            &view(),
            (&target.color_attachment(), &target.depth_attachment()),
            (Some(0), false, &[]),
            scheduler,
        )
        .unwrap()
        .unwrap();
}

#[test]
fn a_same_frame_capture_lends_its_streamed_terrain_to_game_features() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = same_frame_capture_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let lent = crate::compositor::terrain_probe::install(&mut scheduler, &context);
    render_one_capture(&mut renderer, &mut scheduler, &context);
    let lent = lent.answers();
    assert!(!lent.is_empty(), "the probe prepared in the captured frame");
    assert!(
        lent.iter()
            .all(|answer| answer.is_ok_and(|count| count.is_some_and(|count| count > 0))),
        "every segment that prepared the probe was lent the frame's chunks: {lent:?}"
    );
}

#[test]
fn the_depth_prefix_of_a_same_frame_capture_publishes_the_chunks_the_suffix_lends() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = same_frame_capture_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let _lent = crate::compositor::terrain_probe::install(&mut scheduler, &context);
    drop(crate::compositor::terrain_probe::take_publications());
    render_one_capture(&mut renderer, &mut scheduler, &context);
    let published = crate::compositor::terrain_probe::take_publications();
    assert_eq!(
        published.len(),
        2,
        "the depth prefix and the suffix each publish"
    );
    assert!(
        !published[0].owner.same_transaction(&published[1].owner),
        "the two publications belong to two segments of the capture"
    );
    assert!(
        published[0]
            .lent
            .as_ref()
            .is_ok_and(|chunks| chunks.as_ref().is_some_and(|chunks| !chunks.is_empty())),
        "the prefix lends the streamed chunks, not a refusal"
    );
    assert_eq!(
        published[0].lent, published[1].lent,
        "a prefix feature reads the chunks the suffix reads"
    );
}

#[test]
fn a_capture_without_game_features_publishes_no_terrain() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = same_frame_capture_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    drop(crate::compositor::terrain_probe::take_publications());
    render_one_capture(&mut renderer, &mut scheduler, &context);
    let published = crate::compositor::terrain_probe::take_publications();
    assert!(published.is_empty(), "nothing is lent");
}
