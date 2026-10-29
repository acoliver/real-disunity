//! Whole-frame checks that projection jitter is opt-in and reaches the scene pass.
use super::decals::{capture_with_renderer, source};
use super::*;
use crate::compositor::FrameRenderer;
use worldrunner_render_post::{FrameConfig, ProjectionJitter};

fn renderer(context: &GpuContext) -> FrameRenderer {
    FrameRenderer::with_output(
        context.device(),
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        COLOR_FORMAT,
    )
    .unwrap()
}

#[test]
fn jitter_explicitly_off_renders_the_bytes_of_a_renderer_without_the_setting() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut absent = renderer(&context);
    let mut toggled = renderer(&context);
    toggled
        .set_projection_jitter(Some(ProjectionJitter::default()))
        .unwrap();
    toggled.set_projection_jitter(None).unwrap();
    for _ in 0..3 {
        let expected = capture_with_renderer(&context, &mut absent, (&mut gpu, &pipelines), true);
        let actual = capture_with_renderer(&context, &mut toggled, (&mut gpu, &pipelines), true);
        assert_eq!(actual, expected);
    }
    assert!(!toggled.has_temporal_history());
}

#[test]
fn jitter_on_moves_the_drawn_scene_and_publishes_history_when_the_frame_is_submitted() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut plain = renderer(&context);
    let unjittered = capture_with_renderer(&context, &mut plain, (&mut gpu, &pipelines), true);
    let mut jittered = renderer(&context);
    jittered
        .set_projection_jitter(Some(ProjectionJitter::default()))
        .unwrap();
    assert!(!jittered.has_temporal_history());
    let moved = capture_with_renderer(&context, &mut jittered, (&mut gpu, &pipelines), true);
    assert_ne!(moved, unjittered);
    assert!(jittered.has_temporal_history());
}
