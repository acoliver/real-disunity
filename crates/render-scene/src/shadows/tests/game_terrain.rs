//! A game feature on the shadowed route reads the frame's streamed terrain like on the
//! unshadowed one.
use super::*;

#[test]
fn shadowed_frames_lend_their_streamed_terrain_to_game_features() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = super::streaming::multilevel_source();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let lent = crate::compositor::terrain_probe::install(&mut renderer.scheduler, &context);
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for _ in 0..2 {
        render_target(&mut renderer, &context, &view(), &target).unwrap();
    }
    let lent = lent.lock().unwrap();
    assert_eq!(lent.len(), 2, "the probe prepared in both frames");
    assert!(lent[0].is_ok_and(|count| count.is_some_and(|count| count > 0)));
    assert!(
        lent[1].is_ok_and(|count| count.is_some_and(|count| count > 1)),
        "the second frame lends the refined selection: {lent:?}"
    );
}
