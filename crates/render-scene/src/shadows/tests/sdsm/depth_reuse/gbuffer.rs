mod publication;
mod retained_retry;
mod skinned_retry;
mod stock_plan;
mod velocity;

use super::*;
use crate::compositor::FeatureTarget;
use crate::gbuffer::GbufferTarget;
use crate::shadows::tests::deferred::read;
use worldrunner_render_plan::SceneOpaquePath;

fn attachments(frame: &FrameRenderer) -> (AttachmentView, AttachmentView) {
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
    (color, depth)
}

fn render_source() -> RenderScene {
    let base = fitting_source();
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(SceneOpaquePath::GBUFFER_RENDER)
        .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: base.terrain().cloned(),
        effects: Some(effects),
    })
    .unwrap()
}

fn fitted_frame(context: &GpuContext, source: &RenderScene) -> (FrameRenderer, ShadowedScene) {
    let size = [view().viewport().width, view().viewport().height];
    let frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let mut shadows = settings();
    shadows.normal_bias_meters = 0.0;
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        shadows,
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
    (frame, renderer)
}

#[test]
fn gpu_same_frame_fitted_render_preserves_stored_and_lit_geometry() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = render_source();
    let (mut frame, mut renderer) = fitted_frame(&context, &source);
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
    assert_eq!(frame.ssao.as_ref().unwrap().depth, *depth.texture());
    let source_frame = 7;
    let fit = renderer
        .render_depth_fitted_host(
            &context,
            &view(),
            (&color, &depth),
            (Some(source_frame), false, &[]),
            &mut frame,
        )
        .unwrap()
        .expect("a same-frame Render capture adopts its own fit");
    assert_eq!(
        (fit.source_frame, fit.adopted_frame),
        (source_frame, source_frame)
    );
    let g3 = read(
        &context,
        frame
            .feature_target(FeatureTarget::Gbuffer(GbufferTarget::G3))
            .unwrap(),
    );
    let lit_pixels = crate::terrain::deferred_tests::halves(&read(&context, color.texture()));
    assert!(
        g3.chunks_exact(2)
            .zip(lit_pixels.chunks_exact(4))
            .any(|(code, pixel)| {
                u16::from_le_bytes([code[0], code[1]]) != 0
                    && pixel[..3].iter().all(|value| value.is_finite())
                    && pixel[..3].iter().any(|value| *value > 0.0)
            })
    );
}

#[test]
fn gpu_stock_depth_prefix_plan_lists_what_the_fused_producer_completes() {
    use worldrunner_render_post::{PassEntry, PassSlot};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = render_source();
    let (mut frame, mut renderer) = fitted_frame(&context, &source);
    let prefix = frame
        .stock_depth_prefix(context.device(), (&mut renderer.gpu, &renderer.pipelines))
        .unwrap();
    let plan = prefix.plan();
    let entries: Vec<_> = plan
        .passes
        .iter()
        .map(worldrunner_render_post::PassDeclaration::entry)
        .collect();
    assert_eq!(
        entries,
        [
            PassEntry::new(PassSlot::DepthNormalPrepass),
            PassEntry::new(PassSlot::DepthPyramid(0)),
        ],
        "the fused producer is the prepass entry, not a further one"
    );
    let prepass = &plan.passes[0];
    let attached: Vec<_> = prepass
        .color
        .iter()
        .chain(&prepass.additional_colors)
        .map(|attachment| attachment.resource)
        .collect();
    for target in GbufferTarget::ALL {
        let resource = ResourceRef::new(target.resource_name());
        assert!(attached.contains(&resource), "{target:?} is attached");
        assert!(
            plan.views.iter().any(|view| view.resource() == resource),
            "{target:?} has a view in the plan"
        );
    }
}
