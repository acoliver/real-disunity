//! The resident route lends its streamed terrain to the game's features.
use super::*;
use crate::draw::PreparedScene;

#[test]
fn resident_frames_lend_their_streamed_terrain_to_game_features() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        PreparedScene::new(&streamed_scene()).unwrap(),
        &pipelines,
        None,
    )
    .unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let target = target(&context);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let lent = crate::compositor::terrain_probe::install(&mut renderer, &context);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            crate::compositor::encode_resident(
                &mut renderer,
                context.device(),
                encoder,
                (&gpu, &pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
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
