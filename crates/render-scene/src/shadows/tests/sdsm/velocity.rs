use super::*;
use crate::compositor::FrameRenderer;

fn fixture(context: &GpuContext) -> (ShadowedScene, FrameRenderer, SceneView) {
    let source = scene(&[[0, 0, 1000]], 600_000, None);
    let mut parts = super::super::fixture::effects_parts(600_000);
    parts.anti_alias = SceneAntiAlias::taa(900_000);
    let effects = SceneEffects::new(parts).unwrap();
    let source = source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    let scheduler = FrameRenderer::hdr(
        context,
        [8, 8],
        OffscreenTarget::COLOR_FORMAT,
        1,
        worldrunner_render_post::ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let renderer = ShadowedScene::with_taa_velocity(
        context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let view = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -4000, 1500],
            target_millimeters: [0, 0, 0],
        },
        *source.effects().unwrap().camera(),
        crate::Viewport {
            width: 8,
            height: 8,
        },
    )
    .unwrap();
    (renderer, scheduler, view)
}

#[test]
fn same_frame_capture_declares_velocity_attachment() {
    let context = context();
    let (mut renderer, mut scheduler, view) = fixture(&context);
    renderer
        .set_depth_fitting(
            &context,
            Some(crate::shadows::sdsm::SdsmSettings {
                policy: crate::shadows::sdsm::AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        scheduler.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        scheduler.targets().depth().texture(),
    )
    .unwrap();
    let fit = renderer
        .render_depth_fitted_with(&context, &view, (&color, &depth), 0, &mut scheduler)
        .unwrap()
        .unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
}
