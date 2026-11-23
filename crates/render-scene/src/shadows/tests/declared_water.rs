//! A game that draws the water, through the three opaque routes of the shadowed frame. Water
//! over the deferred terrain receives the cascades, so a shadowed frame reads the shadow depth
//! for it. Water that a game draws is drawn by no stock pass and reads nothing, so the frame is
//! the one of the same scene without water: the same colour, the same depth and the same stored
//! and lit pixels, while the stock draws of the same water change the colour. Renderers with and
//! without the declaring game take turns on one shadow scene, whose retained cascades the water
//! never touches.
use super::deferred::{Captured, captured, frame_declaring};
use super::render_path::source;
use super::*;
use crate::compositor::StockReplacement;

/// Encodes one frame of `source` through `path` for each choice in turn, each on a renderer of
/// its own over one shadow scene, and returns what the last frame left.
fn run(
    context: &GpuContext,
    source: &RenderScene,
    path: SceneOpaquePath,
    games: &[StockReplacement],
) -> Captured {
    let view = view();
    let mut shadowed = ShadowedScene::with_target(
        context,
        source,
        super::render_path::front_settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let mut last = None;
    for replaces in games {
        let mut renderer = frame_declaring(context, source, &view, path, *replaces);
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

const ROUTES: [(&str, SceneOpaquePath); 3] = [
    ("forward", SceneOpaquePath::Forward),
    ("observation", SceneOpaquePath::GBUFFER_OBSERVATION),
    ("render", SceneOpaquePath::GBUFFER_RENDER),
];

#[test]
fn gpu_declared_water_leaves_each_shadowed_route_as_a_scene_without_water() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let wet = source(true, true);
    let dry = source(false, true);
    for (name, path) in ROUTES {
        let control = run(&context, &dry, path, &[StockReplacement::NONE]);
        let drawn = run(&context, &wet, path, &[StockReplacement::NONE]);
        let declared = run(&context, &wet, path, &[StockReplacement::WATER]);
        assert!(drawn.colour != control.colour, "{name}: water is visible");
        assert!(declared.colour == control.colour, "{name}: the frame");
        assert!(declared.depth == control.depth, "{name}: depth");
        assert!(declared.g3 == control.g3, "{name}: stored pixels");
        assert!(declared.lit == control.lit, "{name}: lit pixels");
    }
}

#[test]
fn gpu_stock_water_on_a_shadowed_route_is_unchanged_by_a_declaring_renderer_between_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let wet = source(true, true);
    let [stock, declared] = [StockReplacement::NONE, StockReplacement::WATER];
    for (name, path) in ROUTES {
        let fresh = run(&context, &wet, path, &[stock]);
        let switched = run(
            &context,
            &wet,
            path,
            &[stock, declared, stock, declared, stock],
        );
        assert!(switched.colour == fresh.colour, "{name}: the frame");
        assert!(switched.depth == fresh.depth, "{name}: depth");
        assert!(switched.g3 == fresh.g3, "{name}: stored pixels");
        assert!(switched.lit == fresh.lit, "{name}: lit pixels");
    }
}
