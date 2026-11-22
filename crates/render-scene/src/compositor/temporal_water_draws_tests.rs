//! The water-draw switch on a temporal frame. With the stock draws suppressed the frame plans no
//! water pass and the scene colour is the one of a renderer without water. The stock draws are
//! the frames of a renderer built without the setting, whatever was drawn between them.
use super::taa_end_to_end_tests::{OUTPUT, context};
use super::temporal_water_tests::{body, scene_bytes, submit_to};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration, WaterDraws, plan_trace};
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

/// A temporal renderer with, when `water`, a body in front of the wall.
/// `draws` is left to the default when it is `None`.
fn renderer(
    context: &GpuContext,
    water: bool,
    draws: Option<WaterDraws>,
) -> ConfiguredFrameRenderer {
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
    if let Some(draws) = draws {
        settings.water_draws = draws;
    }
    let mut renderer =
        ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap();
    if water {
        renderer
            .renderer_mut()
            .install_water(context, body(false))
            .unwrap();
    }
    renderer
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
fn gpu_suppressed_water_records_no_water_pass_and_still_renders() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut stock = renderer(&context, true, None);
    let drawn = frame(&context, &mut stock, 0);
    assert!(names(&drawn, "TransparentWater"), "{:?}", drawn.passes);

    let mut suppressed = renderer(&context, true, Some(WaterDraws::Suppressed));
    let hidden = frame(&context, &mut suppressed, 0);
    let mut dry = renderer(&context, false, None);
    let control = frame(&context, &mut dry, 0);

    assert!(!names(&hidden, "TransparentWater"), "{:?}", hidden.passes);
    assert_eq!(hidden.passes, control.passes, "the plan");
    assert!(drawn.scene != control.scene, "the stock water is visible");
    assert!(hidden.scene == control.scene, "the frame renders the scene");
}

#[test]
fn gpu_stock_water_draws_match_a_renderer_built_without_the_setting() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut reference = renderer(&context, true, None);
    let mut explicit = renderer(&context, true, Some(WaterDraws::Stock));
    for index in 0..4 {
        let expected = frame(&context, &mut reference, index);
        let actual = frame(&context, &mut explicit, index);
        assert_eq!(actual.passes, expected.passes, "frame {index}");
        assert!(actual.scene == expected.scene, "frame {index}: scene");
    }
}

#[test]
fn gpu_switching_water_draws_restores_the_stock_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut reference = renderer(&context, true, None);
    let frames: Vec<Frame> = (0..3).map(|i| frame(&context, &mut reference, i)).collect();

    let mut switched = renderer(&context, true, None);
    let first = frame(&context, &mut switched, 0);
    assert!(first.scene == frames[0].scene, "the first frame's scene");
    switched
        .renderer_mut()
        .force_water_draws(WaterDraws::Suppressed);
    let hidden = frame(&context, &mut switched, 1);
    assert!(!names(&hidden, "TransparentWater"), "{:?}", hidden.passes);
    switched.renderer_mut().force_water_draws(WaterDraws::Stock);
    let again = frame(&context, &mut switched, 2);
    assert_eq!(again.passes, frames[2].passes, "the plan");
    assert!(again.scene == frames[2].scene, "the scene");
}
