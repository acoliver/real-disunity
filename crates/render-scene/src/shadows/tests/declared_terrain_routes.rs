//! A game that draws the terrain, through the shadowed frame. No stock camera pass draws the
//! terrain, so the camera depth is the one of the same scene without a terrain, while the stock
//! draws of the same terrain change it. The stock terrain stays in the shadow cascades.
use super::shadowed_capture::{capture_drawing, scheduler, with_prepass};
use super::*;
use crate::compositor::StockReplacement;

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
fn gpu_declared_terrain_leaves_the_shadowed_camera_depth_as_a_terrain_free_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let terrain = with_prepass(&super::terrain::terrain_scene(
        &scene(&[[0, 0, 1_000]], 600_000, None),
        true,
        false,
    ));
    let capture = |source: &RenderScene, replaces| {
        capture_drawing(&context, source, &view(), replaces).unwrap()
    };
    let control = capture(&without_terrain(&terrain), StockReplacement::NONE);
    let drawn = capture(&terrain, StockReplacement::NONE);
    let declared = capture(&terrain, StockReplacement::TERRAIN_SURFACE);
    assert!(
        drawn.depth != control.depth,
        "the stock terrain shows in depth"
    );
    assert!(drawn.colour != control.colour, "the stock terrain is lit");
    // The frame is not compared with the control: the terrain's shadows may change it.
    assert!(declared.depth == control.depth, "depth");
    assert!(declared.colour != drawn.colour, "the terrain is not lit");
}

/// The depth texels of the four shadow cascades after one frame of `source`, with a game that
/// draws what `replaces` says.
fn cascade_depths(source: &RenderScene, replaces: StockReplacement) -> Vec<Vec<f32>> {
    let context = context();
    let mut renderer = shadowed(&context, source, settings()).unwrap();
    let mut scheduler = scheduler(&context, replaces);
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    super::refresh::depth_layers(&context, &renderer)
}

#[test]
fn gpu_declared_terrain_still_writes_the_stock_cascade_depth() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let terrain =
        super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), true, false);
    let stock = cascade_depths(&terrain, StockReplacement::NONE);
    let declared = cascade_depths(&terrain, StockReplacement::TERRAIN_SURFACE);
    let bare = cascade_depths(&without_terrain(&terrain), StockReplacement::NONE);
    assert!(
        stock != bare,
        "the terrain casts into the cascades, so removing it changes a texel"
    );
    assert!(declared == stock, "the declared terrain casts as stock");
}
