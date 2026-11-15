//! Depth fitting without SSAO or SSR still draws the prepass; Opaque must reuse its depth.
mod gbuffer;

use super::*;
use crate::compositor::FrameRenderer;
use crate::renderer::depth::DepthMode;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_post::{Readback, ToneMap};

fn fitting_source() -> RenderScene {
    let base = scene(&[[0, 0, 1000], [400, 500, 800]], 600_000, None);
    let effects = SceneEffects::new(fixture::effects_parts(600_000))
        .unwrap()
        .with_depth_fitting(SceneDepthFitting {
            near_step_mm: 100,
            far_step_mm: 1000,
            minimum_span_mm: 1000,
            minimum_span_policy: SceneDepthMinimumSpan::ExtendNear,
            active_maximum_mm: None,
            splits: SceneDepthSplits::Practical {
                lambda_micro: 500_000,
            },
            adoption: SceneDepthAdoption::FrameIndexed {
                latency_frames: 2,
                contraction_frames: 0,
            },
        })
        .unwrap();
    assert!(effects.ssao().is_none() && effects.ssr().is_none());
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

/// Renders one same-frame fitted capture and returns the scene colour. `independent` gives the
/// frame its own Opaque depth, the reference the paired depth must reproduce.
fn fitted_color(context: &GpuContext, independent: bool) -> Vec<u8> {
    let source = fitting_source();
    let size = [view().viewport().width, view().viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    if independent {
        frame.independent_depth_for_test(context.device());
    }
    let mut renderer = ShadowedScene::with_target(
        context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let metadata = frame
        .ssao
        .as_ref()
        .expect("depth fitting installs the frame's prepass metadata");
    assert_eq!(metadata.depth == *depth.texture(), !independent);
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_with(context, &view(), (&color, &depth), 0, &mut frame)
        .unwrap()
        .expect("a same-frame capture adopts its own fit");
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
        for slot in [
            worldrunner_render_post::PassSlot::DepthNormalPrepass,
            worldrunner_render_post::PassSlot::DepthPyramid(0),
        ] {
            assert_eq!(
                passes.iter().filter(|&&pass| pass == slot).count(),
                1,
                "duplicate or missing producer: {slot:?}"
            );
        }
    });
    assert_eq!(
        frame.forward_depth(&renderer.gpu, &depth),
        if independent {
            DepthMode::Independent
        } else {
            DepthMode::Reuse
        }
    );
    let mut readback = Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        16 * 1024 * 1024,
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    readback.encode(&mut encoder, color.texture()).unwrap();
    let submission = encoder.finish().submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}

#[test]
fn gpu_depth_fitting_without_screen_effects_reuses_its_prepass_depth_in_opaque() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let reuse = fitted_color(&context, false);
    assert!(reuse.chunks_exact(8).any(|pixel| pixel[..6] != [0; 6]));
    assert!(
        reuse == fitted_color(&context, true),
        "paired depth must shade what independent depth shades"
    );
}
