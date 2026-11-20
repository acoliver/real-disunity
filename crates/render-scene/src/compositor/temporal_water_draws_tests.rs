//! The water-draw switch on a frame that carries the temporal reactive mask. With the stock draws
//! suppressed the frame plans neither the water pass nor the mask pass that follows it, the mask
//! stays at the host's clear and the scene colour is the one of a renderer without water. The
//! stock draws are the frames of a renderer built without the setting, whatever was drawn
//! between them.
use super::taa_end_to_end_tests::{OUTPUT, context};
use super::temporal_water_tests::{body, read_mask, scene_bytes, submit_to};
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
    mask: Vec<[u8; 2]>,
}

/// A temporal renderer with the mask producer on and, when `water`, a body in front of the wall.
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
    let mut settings = RendererConfiguration::legacy(OUTPUT_EXTENT, OUTPUT);
    settings.frame = FrameConfig::new(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent(render);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
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
        let frame = renderer.renderer_mut();
        frame.install_water(context, body(false)).unwrap();
        frame.set_water_reactive_mask(true);
    }
    renderer
}

fn frame(context: &GpuContext, renderer: &mut ConfiguredFrameRenderer, index: usize) -> Frame {
    plan_trace::take();
    submit_to(
        context,
        renderer.renderer_mut(),
        (0.25 * f32::from(u8::try_from(index).unwrap()), index % 3),
        (true, None),
    );
    let passes = plan_trace::take()
        .into_iter()
        .flat_map(|plan| plan.passes)
        .collect();
    let mask = read_mask(
        context,
        renderer.targets().temporal_mask_texture().expect("a mask"),
    );
    Frame {
        passes,
        scene: scene_bytes(context, renderer.renderer_mut()),
        mask,
    }
}

/// What a host does first in every frame: zero the temporal mask before any producer writes it.
fn clear_mask(context: &GpuContext, renderer: &mut ConfiguredFrameRenderer) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer.targets().encode_mask_clear(&mut encoder);
    encoder.finish().submit().unwrap();
}

fn names(frame: &Frame, slot: &str) -> bool {
    frame
        .passes
        .iter()
        .any(|pass| pass.starts_with(&format!("{slot}#")))
}

#[test]
fn gpu_suppressed_water_records_no_water_pass_and_no_mask_and_still_renders() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut stock = renderer(&context, true, None);
    let drawn = frame(&context, &mut stock, 0);
    assert!(names(&drawn, "TransparentWater"), "{:?}", drawn.passes);
    assert!(names(&drawn, "TransparentWaterMask"), "{:?}", drawn.passes);
    assert!(drawn.mask.iter().any(|texel| texel[0] != 0));

    let mut suppressed = renderer(&context, true, Some(WaterDraws::Suppressed));
    let hidden = frame(&context, &mut suppressed, 0);
    let mut dry = renderer(&context, false, None);
    let control = frame(&context, &mut dry, 0);

    assert!(!names(&hidden, "TransparentWater"), "{:?}", hidden.passes);
    assert!(
        !names(&hidden, "TransparentWaterMask"),
        "{:?}",
        hidden.passes
    );
    assert!(hidden.mask.iter().all(|texel| *texel == [0, 0]), "the mask");
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
        assert!(actual.mask == expected.mask, "frame {index}: mask");
    }
}

#[test]
fn gpu_switching_water_draws_leaves_no_stale_mask_and_restores_the_stock_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut reference = renderer(&context, true, None);
    // The reference host zeroes the mask before every frame, as the producer contract asks, so
    // the mask of its frame 2 is the one frame's water and not the union of three jittered ones.
    // The switched renderer gets no host clear: it has to clear the water it drew itself.
    let frames: Vec<Frame> = (0..3)
        .map(|i| {
            clear_mask(&context, &mut reference);
            frame(&context, &mut reference, i)
        })
        .collect();

    let mut switched = renderer(&context, true, None);
    let first = frame(&context, &mut switched, 0);
    assert!(first.mask == frames[0].mask, "the first frame's mask");
    switched
        .renderer_mut()
        .force_water_draws(WaterDraws::Suppressed);
    let hidden = frame(&context, &mut switched, 1);
    assert!(!names(&hidden, "TransparentWater"), "{:?}", hidden.passes);
    assert!(
        hidden.mask.iter().all(|texel| *texel == [0, 0]),
        "the previous frame's water mask is gone"
    );
    switched.renderer_mut().force_water_draws(WaterDraws::Stock);
    let again = frame(&context, &mut switched, 2);
    assert_eq!(again.passes, frames[2].passes, "the plan");
    assert!(again.scene == frames[2].scene, "the scene");
    assert!(again.mask == frames[2].mask, "the mask");
}
