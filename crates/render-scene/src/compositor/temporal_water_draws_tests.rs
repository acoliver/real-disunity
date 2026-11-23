//! A game that draws the water, on a temporal frame. The frame plans no water pass and the scene
//! colour is the one of a renderer without water. A game that declares nothing leaves the frames
//! of a renderer without a game, and a renderer rebuilt without the declaring game draws the
//! stock frames again, whatever was drawn between them.
use super::taa_end_to_end_tests::{OUTPUT, context};
use super::temporal_water_tests::{body, scene_bytes, submit_to};
use crate::compositor::{
    ConfiguredFrameRenderer, GamePasses, RendererConfiguration, StockReplacement, declaring_stock,
    plan_trace,
};
use crate::device::GpuContext;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{FrameConfig, HdrPostSettings, PostResolve, ToneMap};

const OUTPUT_EXTENT: [u32; 2] = [64, 64];

/// What one frame submitted and left behind.
struct Frame {
    passes: Vec<String>,
    scene: Vec<u8>,
}

/// The settings of a temporal renderer with, when `water`, a body in front of the wall and the
/// game inventory `game`.
fn settings(water: bool, game: GamePasses) -> RendererConfiguration {
    let render = RenderScale::new(670_000)
        .unwrap()
        .extent(OUTPUT_EXTENT)
        .unwrap();
    let mut settings = RendererConfiguration::new(OUTPUT_EXTENT, OUTPUT, ToneMap::Reinhard);
    settings.frame = FrameConfig::new(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent(render);
    settings.post = HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    settings.game_passes = game;
    if water {
        settings.water = vec![body(false)];
    }
    settings
}

fn renderer(context: &GpuContext, water: bool, game: GamePasses) -> ConfiguredFrameRenderer {
    ConfiguredFrameRenderer::new(context, settings(water, game), AllocationBudget(1 << 28)).unwrap()
}

fn frame(context: &GpuContext, renderer: &mut ConfiguredFrameRenderer, index: usize) -> Frame {
    plan_trace::take();
    submit_to(
        context,
        renderer.renderer_mut(),
        (0.25 * f32::from(u8::try_from(index).unwrap()), index % 3),
        true,
    );
    let passes = plan_trace::take()
        .into_iter()
        .flat_map(|plan| plan.passes)
        .collect();
    Frame {
        passes,
        scene: scene_bytes(context, renderer.renderer_mut()),
    }
}

fn names(frame: &Frame, slot: &str) -> bool {
    frame
        .passes
        .iter()
        .any(|pass| pass.starts_with(&format!("{slot}#")))
}

#[test]
fn gpu_declared_water_records_no_water_pass_and_still_renders() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut stock = renderer(&context, true, GamePasses::new());
    let drawn = frame(&context, &mut stock, 0);
    assert!(names(&drawn, "TransparentWater"), "{:?}", drawn.passes);

    let mut declared = renderer(&context, true, declaring_stock(StockReplacement::WATER));
    let hidden = frame(&context, &mut declared, 0);
    let mut dry = renderer(&context, false, GamePasses::new());
    let control = frame(&context, &mut dry, 0);

    assert!(!names(&hidden, "TransparentWater"), "{:?}", hidden.passes);
    assert_eq!(hidden.passes, control.passes, "the plan");
    assert!(drawn.scene != control.scene, "the stock water is visible");
    assert!(hidden.scene == control.scene, "the frame renders the scene");
}

#[test]
fn gpu_a_game_that_declares_nothing_leaves_the_stock_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut reference = renderer(&context, true, GamePasses::new());
    let mut explicit = renderer(&context, true, declaring_stock(StockReplacement::NONE));
    for index in 0..4 {
        let expected = frame(&context, &mut reference, index);
        let actual = frame(&context, &mut explicit, index);
        assert_eq!(actual.passes, expected.passes, "frame {index}");
        assert!(actual.scene == expected.scene, "frame {index}: scene");
    }
}

/// Frame `index` of `renderer` after it is rebuilt from `settings`.
fn rebuilt_frame(
    context: &GpuContext,
    renderer: &mut ConfiguredFrameRenderer,
    settings: RendererConfiguration,
    index: usize,
) -> Frame {
    renderer.reconfigure(context, settings).unwrap();
    frame(context, renderer, index)
}

#[test]
fn gpu_rebuilding_without_the_declaring_game_restores_the_stock_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let stock = || settings(true, GamePasses::new());
    let declared = || settings(true, declaring_stock(StockReplacement::WATER));

    // A renderer that is rebuilt as stock twice is the reference for one rebuilt with the
    // declaring game in between: the declaration leaves nothing behind.
    let mut reference = renderer(&context, true, GamePasses::new());
    let first = frame(&context, &mut reference, 0);
    let drawn = rebuilt_frame(&context, &mut reference, stock(), 1);
    assert!(names(&drawn, "TransparentWater"), "{:?}", drawn.passes);
    let expected = rebuilt_frame(&context, &mut reference, stock(), 2);

    let mut switched = renderer(&context, true, GamePasses::new());
    let start = frame(&context, &mut switched, 0);
    assert!(start.scene == first.scene, "the first frame's scene");
    let hidden = rebuilt_frame(&context, &mut switched, declared(), 1);
    assert!(!names(&hidden, "TransparentWater"), "{:?}", hidden.passes);
    let again = rebuilt_frame(&context, &mut switched, stock(), 2);
    assert_eq!(again.passes, expected.passes, "the plan");
    assert!(again.scene == expected.scene, "the scene");
}
