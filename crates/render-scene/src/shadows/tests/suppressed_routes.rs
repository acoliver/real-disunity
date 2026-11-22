//! The terrain-draw switch through the shadowed frame. Suppressed terrain is drawn by no pass, so
//! the camera result is the one of the same scene without a terrain: the same depth and the same
//! frame, while the stock draws of the same terrain change both. `CastersOnly` leaves the camera
//! depth of `Suppressed`, and keeps the stock terrain in the shadow cascades.
use super::shadowed_capture::{capture_drawing, with_prepass};
use super::*;
use crate::compositor::TerrainDraws;

/// `source` with its terrain taken out and everything else as it was.
fn without_terrain(source: &RenderScene) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: source.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn gpu_suppressed_terrain_leaves_the_shadowed_frame_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(&super::terrain::terrain_scene(
        &scene(&[[0, 0, 1_000]], 600_000, None),
        true,
        false,
    ));
    let capture =
        |source: &RenderScene, draws| capture_drawing(&context, source, &view(), draws).unwrap();
    let control = capture(&without_terrain(&terrain), TerrainDraws::Stock);
    let drawn = capture(&terrain, TerrainDraws::Stock);
    let suppressed = capture(&terrain, TerrainDraws::Suppressed);
    assert!(
        drawn.depth != control.depth,
        "the stock terrain shows in depth"
    );
    assert!(drawn.colour != control.colour, "the stock terrain is lit");
    assert!(suppressed.depth == control.depth, "depth");
    assert!(suppressed.colour == control.colour, "the frame");
}

#[test]
fn gpu_casters_only_terrain_leaves_the_shadowed_camera_depth_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(&super::terrain::terrain_scene(
        &scene(&[[0, 0, 1_000]], 600_000, None),
        true,
        false,
    ));
    let capture =
        |source: &RenderScene, draws| capture_drawing(&context, source, &view(), draws).unwrap();
    let control = capture(&without_terrain(&terrain), TerrainDraws::Stock);
    let drawn = capture(&terrain, TerrainDraws::Stock);
    let casters = capture(&terrain, TerrainDraws::CastersOnly);
    // The frames are not compared with the control: the terrain's shadows may change them.
    assert!(casters.depth == control.depth, "depth");
    assert!(casters.colour != drawn.colour, "the terrain is not lit");
}

/// The depth texels of the four shadow cascades after one frame of `source` with `draws`.
fn cascade_depths(source: &RenderScene, draws: TerrainDraws) -> Vec<Vec<f32>> {
    let context = context();
    let mut renderer = ShadowedScene::new(&context, source, settings()).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    scheduler.force_terrain_draws(draws);
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    super::refresh::depth_layers(&context, &renderer)
}

#[test]
fn gpu_casters_only_terrain_writes_the_stock_cascade_depth_and_suppressed_terrain_does_not() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let terrain =
        super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), true, false);
    let stock = cascade_depths(&terrain, TerrainDraws::Stock);
    let casters = cascade_depths(&terrain, TerrainDraws::CastersOnly);
    let suppressed = cascade_depths(&terrain, TerrainDraws::Suppressed);
    assert!(
        stock != suppressed,
        "the terrain casts into the cascades, so removing it changes a texel"
    );
    assert!(casters == stock, "the casters only terrain casts as stock");
    assert!(casters != suppressed, "and not as suppressed terrain");
}
