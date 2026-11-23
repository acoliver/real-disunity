//! The terrain-draw switch through the two G-buffer routes of the shadowed frame. Suppressed
//! terrain is drawn by no G-buffer producer, so the frame is the one of the same scene without a
//! terrain: the same depth, the same stored pixels and the same lit pixels, while the stock draws
//! of the same terrain change each of them. `CastersOnly` leaves the camera result of
//! `Suppressed` in depth and stored pixels, and keeps the stock terrain in the shadow cascades.
use super::deferred::{capture_drawing, with_prepass};
use super::render_path::front_settings;
use super::*;
use crate::compositor::TerrainDraws;
use crate::offscreen::OffscreenTarget;

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
fn gpu_suppressed_terrain_leaves_the_observation_route_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(
        &super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), true, false),
        true,
    );
    let route = SceneOpaquePath::GBUFFER_OBSERVATION;
    let capture = |source: &RenderScene, draws| {
        capture_drawing(&context, source, &view(), route, (draws, front_settings())).unwrap()
    };
    let control = capture(&without_terrain(&terrain), TerrainDraws::Stock);
    let drawn = capture(&terrain, TerrainDraws::Stock);
    let suppressed = capture(&terrain, TerrainDraws::Suppressed);
    assert!(
        drawn.depth != control.depth,
        "the stock terrain shows in depth"
    );
    assert!(drawn.g3 != control.g3, "the stock terrain stores pixels");
    assert!(suppressed.depth == control.depth, "depth");
    assert!(suppressed.g3 == control.g3, "stored pixels");
    assert!(suppressed.lit == control.lit, "lit pixels");
    assert!(suppressed.colour == control.colour, "the forward frame");
}

#[test]
fn gpu_suppressed_terrain_leaves_the_render_route_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(
        &super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), true, false),
        true,
    );
    let route = SceneOpaquePath::GBUFFER_RENDER;
    let capture = |source: &RenderScene, draws| {
        capture_drawing(&context, source, &view(), route, (draws, front_settings())).unwrap()
    };
    let control = capture(&without_terrain(&terrain), TerrainDraws::Stock);
    let drawn = capture(&terrain, TerrainDraws::Stock);
    let suppressed = capture(&terrain, TerrainDraws::Suppressed);
    assert!(
        drawn.depth != control.depth,
        "the stock terrain shows in depth"
    );
    assert!(drawn.colour != control.colour, "the stock terrain is lit");
    assert!(suppressed.depth == control.depth, "depth");
    assert!(suppressed.colour == control.colour, "lit pixels");
}

#[test]
fn gpu_casters_only_terrain_leaves_the_observation_camera_result_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(
        &super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), true, false),
        true,
    );
    let route = SceneOpaquePath::GBUFFER_OBSERVATION;
    let capture = |source: &RenderScene, draws| {
        capture_drawing(&context, source, &view(), route, (draws, front_settings())).unwrap()
    };
    let control = capture(&without_terrain(&terrain), TerrainDraws::Stock);
    let casters = capture(&terrain, TerrainDraws::CastersOnly);
    // The lit pixels are not compared: the terrain's shadows may legitimately change them.
    assert!(casters.depth == control.depth, "depth");
    assert!(casters.g3 == control.g3, "stored pixels");
}

#[test]
fn gpu_casters_only_terrain_leaves_the_render_camera_result_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(
        &super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), true, false),
        true,
    );
    let route = SceneOpaquePath::GBUFFER_RENDER;
    let capture = |source: &RenderScene, draws| {
        capture_drawing(&context, source, &view(), route, (draws, front_settings())).unwrap()
    };
    let control = capture(&without_terrain(&terrain), TerrainDraws::Stock);
    let drawn = capture(&terrain, TerrainDraws::Stock);
    let casters = capture(&terrain, TerrainDraws::CastersOnly);
    assert!(casters.depth == control.depth, "depth");
    assert!(casters.colour != drawn.colour, "the terrain is not lit");
}

/// The depth texels of the four shadow cascades after one frame of `source` with `draws`.
fn cascade_depths(source: &RenderScene, draws: TerrainDraws) -> Vec<Vec<f32>> {
    let context = context();
    let mut renderer = shadowed(&context, source, settings()).unwrap();
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
