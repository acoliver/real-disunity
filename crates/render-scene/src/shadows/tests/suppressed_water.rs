//! The water-draw switch through the shadowed frame. Water over the terrain receives the
//! cascades, so a shadowed frame reads the shadow depth for it. Suppressed water is drawn by no
//! pass and reads nothing, so the frame is the one of the same scene without water: the same
//! colour and the same depth, while the stock draws of the same water change the colour. The
//! choice is made frame by frame on one renderer and one shadow scene, whose retained cascades
//! the water never touches.
use super::shadowed_capture::{Captured, captured, frame, source};
use super::*;
use crate::compositor::WaterDraws;

/// Encodes one frame of `source` for each choice in turn on one renderer and one shadow scene,
/// and returns what the last frame left.
fn run(context: &GpuContext, source: &RenderScene, draws: &[WaterDraws]) -> Captured {
    let view = view();
    let mut renderer = frame(context, source, &view);
    let mut shadowed = ShadowedScene::with_target(
        context,
        source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    for choice in draws {
        renderer.force_water_draws(*choice);
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
    }
    captured(context, &renderer)
}

#[test]
fn gpu_suppressed_water_leaves_the_shadowed_frame_as_a_scene_without_water() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let wet = source(true);
    let dry = source(false);
    let control = run(&context, &dry, &[WaterDraws::Stock]);
    let drawn = run(&context, &wet, &[WaterDraws::Stock]);
    let suppressed = run(&context, &wet, &[WaterDraws::Suppressed]);
    assert!(drawn.colour != control.colour, "water is visible");
    assert!(suppressed.colour == control.colour, "the frame");
    assert!(suppressed.depth == control.depth, "depth");
}

#[test]
fn gpu_stock_water_on_the_shadowed_frame_is_unchanged_by_the_setting_between_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let wet = source(true);
    let [stock, suppressed] = [WaterDraws::Stock, WaterDraws::Suppressed];
    let fresh = run(&context, &wet, &[stock]);
    let switched = run(
        &context,
        &wet,
        &[stock, suppressed, stock, suppressed, stock],
    );
    assert!(switched.colour == fresh.colour, "the frame");
    assert!(switched.depth == fresh.depth, "depth");
}
