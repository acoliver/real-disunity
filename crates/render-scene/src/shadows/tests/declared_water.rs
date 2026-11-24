//! A game that draws the water, through the shadowed frame. Water over the terrain receives the
//! cascades, so a shadowed frame reads the shadow depth for it. Water that a game draws is drawn
//! by no stock pass and reads nothing, so the frame is the one of the same scene without water:
//! the same colour and the same depth, while the stock draws of the same water change the colour.
//! Renderers with and without the declaring game take turns on one shadow scene, whose retained
//! cascades the water never touches.
use super::shadowed_capture::{Captured, captured, frame_declaring, source};
use super::*;
use crate::compositor::StockReplacement;

/// Encodes one frame of `source` for each choice in turn, each on a renderer of its own over one
/// shadow scene, and returns what the last frame left.
fn run(context: &GpuContext, source: &RenderScene, games: &[StockReplacement]) -> Captured {
    let view = view();
    let mut shadowed = ShadowedScene::new(
        context,
        PreparedScene::new(source).unwrap(),
        settings(),
        ShadowedTarget::new(wgpu::TextureFormat::Rgba16Float, 1),
    )
    .unwrap();
    let mut last = None;
    for replaces in games {
        let mut renderer = frame_declaring(context, source, &view, *replaces);
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut output = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        shadowed
            .encode_with(
                context,
                &view,
                (&color, &depth),
                &mut encoder,
                &mut renderer,
            )
            .unwrap();
        renderer
            .encode_output(&mut encoder, output.color_view())
            .unwrap();
        output.encode_readback(&mut encoder).unwrap();
        let submission = encoder.finish().submit().unwrap();
        output.read_encoded(context, submission).unwrap();
        last = Some(captured(context, &renderer));
    }
    last.expect("at least one frame")
}

#[test]
fn gpu_declared_water_leaves_the_shadowed_frame_as_a_scene_without_water() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let wet = source(true);
    let dry = source(false);
    let control = run(&context, &dry, &[StockReplacement::NONE]);
    let drawn = run(&context, &wet, &[StockReplacement::NONE]);
    let declared = run(&context, &wet, &[StockReplacement::WATER]);
    assert!(drawn.colour != control.colour, "water is visible");
    assert!(declared.colour == control.colour, "the frame");
    assert!(declared.depth == control.depth, "depth");
}

#[test]
fn gpu_stock_water_on_the_shadowed_frame_is_unchanged_by_a_declaring_renderer_between_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let wet = source(true);
    let [stock, declared] = [StockReplacement::NONE, StockReplacement::WATER];
    let fresh = run(&context, &wet, &[stock]);
    let switched = run(&context, &wet, &[stock, declared, stock, declared, stock]);
    assert!(switched.colour == fresh.colour, "the frame");
    assert!(switched.depth == fresh.depth, "depth");
}
