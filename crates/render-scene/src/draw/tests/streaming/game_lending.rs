//! The resident route lends its streamed terrain to the game's features.
use super::*;
use crate::test_support::upload_scene;

#[test]
fn resident_frames_lend_their_streamed_terrain_to_game_features() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(
        context.device(),
        context.submission(),
        &streamed_scene(),
        &pipelines,
    )
    .unwrap();
    let frame = frame_from(pose());
    gpu.set_frame(context.queue(), frame);
    let target = target(&context);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let lent = crate::compositor::terrain_probe::install(&mut renderer, &context);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encoding().encode_scene(
                context.device(),
                encoder,
                (&mut gpu, &pipelines),
                crate::compositor::ColorDepth {
                    color: &target.color_attachment(),
                    depth: &target.depth_attachment(),
                },
                frame,
            )
        })
        .unwrap();
    prepared.cancel();
    let lent = lent.answers();
    assert_eq!(lent.len(), 1);
    assert!(
        lent[0].is_ok_and(|count| count.is_some_and(|count| count > 0)),
        "{lent:?}"
    );
}
