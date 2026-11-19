//! The terrain-draw switch through the two G-buffer routes of the shadowed frame. Suppressed
//! terrain is drawn by no G-buffer producer, so the frame is the one of the same scene without a
//! terrain: the same depth, the same stored pixels and the same lit pixels, while the stock draws
//! of the same terrain change each of them.
use super::deferred::{capture_drawing, with_prepass};
use super::render_path::front_settings;
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
