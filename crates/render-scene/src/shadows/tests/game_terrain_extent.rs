//! A depth prefix planned from metadata of another size than the colour target cannot give the
//! two segments of a frame different terrain chunks: the frame's terrain selection is made once,
//! for one raster extent, and a second extent is refused.
use super::*;
use crate::compositor::{FrameTerrainError, terrain_probe};
use crate::draw::TerrainChunkViewError;
use crate::frame::FrameUniform;
use crate::offscreen::OffscreenTarget;
use crate::scene_metadata::SceneMetadata;
use worldrunner_render_post::FrameError;

const FULL: [u32; 2] = [257, 241];
const HALF: [u32; 2] = [128, 120];

fn streamed_renderer(context: &GpuContext) -> ShadowedScene {
    let source = super::streaming::multilevel_source();
    shadowed(context, &source, settings()).unwrap()
}

/// One transaction: the stock depth prefix planned from caller metadata of `extent`, then the
/// suffix drawn into a colour target of the view's size.
fn render_prefix_then_suffix(
    renderer: &mut ShadowedScene,
    scheduler: &mut crate::compositor::FrameRenderer,
    context: &GpuContext,
    extent: [u32; 2],
) -> Result<(), ShadowError> {
    let metadata = SceneMetadata::new(context.device(), extent, crate::ssr::neutral_ao()).unwrap();
    let frame = FrameUniform::new(&view(), Some(&renderer.effects)).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let (color, depth) = (target.color_attachment(), target.depth_attachment());
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            scheduler.encode_depth_prefix(
                context.device(),
                encoder,
                (&mut renderer.gpu, &renderer.pipelines),
                &frame,
                &metadata,
            )?;
            let attachments = crate::compositor::ColorDepth {
                color: &color,
                depth: &depth,
            };
            let input = crate::shadows::frame::FitInput {
                scheduler,
                fitting: None,
            };
            renderer.encode_fitted(context, &view(), attachments, encoder, input)
        })
        .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
    prepared.submit()?;
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    Ok(())
}

fn lent_chunks(published: &terrain_probe::Published) -> bool {
    published
        .lent
        .as_ref()
        .is_ok_and(|chunks| chunks.as_ref().is_some_and(|chunks| !chunks.is_empty()))
}

#[test]
fn a_prefix_and_suffix_with_equal_extents_lend_equal_chunks() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = streamed_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let lent = terrain_probe::install(&mut scheduler, &context);
    drop(terrain_probe::take_publications());
    render_prefix_then_suffix(&mut renderer, &mut scheduler, &context, FULL).unwrap();
    let published = terrain_probe::take_publications();
    assert_eq!(published.len(), 2, "the prefix and the suffix each publish");
    assert!(published[0].owner.same_transaction(&published[1].owner));
    assert!(lent_chunks(&published[0]));
    assert_eq!(published[0].lent, published[1].lent);
    let lent = lent.answers();
    assert_eq!(lent.len(), 1, "the probe prepared once, in the suffix");
    assert!(lent[0].is_ok_and(|count| count.is_some_and(|count| count > 0)));
}

#[test]
fn a_suffix_whose_extent_differs_from_its_prefix_fails_the_frame_before_it_lends() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = streamed_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let lent = terrain_probe::install(&mut scheduler, &context);
    drop(terrain_probe::take_publications());
    let refused = render_prefix_then_suffix(&mut renderer, &mut scheduler, &context, HALF);
    assert!(
        matches!(
            refused,
            Err(ShadowError::Schedule(
                FrameError::StreamingSelectionConflict
            ))
        ),
        "the suffix's terrain preparation refuses a second extent: {refused:?}"
    );
    let published = terrain_probe::take_publications();
    assert_eq!(published.len(), 1, "only the prefix published");
    assert!(lent_chunks(&published[0]));
    assert!(
        lent.answers().is_empty(),
        "no feature prepared in the refused frame"
    );
}

#[test]
fn a_second_publication_with_another_extent_in_one_frame_lends_a_typed_refusal() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let renderer = streamed_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let _lent = terrain_probe::install(&mut scheduler, &context);
    drop(terrain_probe::take_publications());
    let frame = FrameUniform::new(&view(), Some(&renderer.effects)).unwrap();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer
                .gpu
                .prepare_terrain_geometry(context.device(), encoder, &frame, HALF)?;
            scheduler.publish_game_terrain(&renderer.gpu, &frame, HALF);
            scheduler.publish_game_terrain(&renderer.gpu, &frame, FULL);
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.cancel();
    let published = terrain_probe::take_publications();
    assert_eq!(published.len(), 2);
    assert!(lent_chunks(&published[0]));
    assert_eq!(
        published[1].lent,
        Err(FrameTerrainError::View(TerrainChunkViewError::ViewMismatch))
    );
}

#[test]
fn the_frame_after_a_refused_extent_lends_its_terrain_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut renderer = streamed_renderer(&context);
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let lent = terrain_probe::install(&mut scheduler, &context);
    assert!(render_prefix_then_suffix(&mut renderer, &mut scheduler, &context, HALF).is_err());
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    let lent = lent.answers();
    assert_eq!(lent.len(), 1, "the probe prepared in the second frame only");
    assert!(
        lent[0].is_ok_and(|count| count.is_some_and(|count| count > 0)),
        "the next frame is unaffected: {lent:?}"
    );
}
