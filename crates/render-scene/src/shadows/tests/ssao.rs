use super::*;
use crate::compositor::FrameRenderer;
use worldrunner_render_post::{FrameError, ToneMap};

fn source() -> RenderScene {
    let base = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(SceneSsao {
        receiver: worldrunner_render_plan::SceneSsaoReceiver::Nearest,
        depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: 1_000_000,
        sample_count: 32,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    });
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(SceneEffects::new(parts).unwrap()),
    })
    .unwrap()
}

fn encode(
    context: &GpuContext,
    resident: &mut ShadowedScene,
    frame: &mut FrameRenderer,
) -> Result<(), ShadowError> {
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
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let result = resident.encode_with(
        context,
        &view(),
        (&color, &depth),
        &mut encoder,
        Some(frame),
    );
    encoder.finish().submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    result
}

#[test]
fn shadowed_ssao_requires_exact_profile_settings_before_preparation() {
    let context = context();
    let source = source();
    let size = [view().viewport().width, view().viewport().height];
    let mut resident = ShadowedScene::with_target(
        &context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let original = source.effects().unwrap().ssao().unwrap();
    let before = resident.gpu.view_stats().clone();
    for changed in crate::ssao::tests::differing_settings(*original) {
        let mut effects = fixture::effects_parts(600_000);
        effects.ssao = Some(changed);
        let effects = SceneEffects::new(effects).unwrap();
        let mut frame = FrameRenderer::hdr(
            &context,
            size,
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
        assert!(matches!(
            encode(&context, &mut resident, &mut frame),
            Err(ShadowError::Schedule(FrameError::SsaoSettings(
                "SSAO resident/profile settings mismatch"
            )))
        ));
        assert_eq!(resident.gpu.view_stats(), before);
        assert_eq!(resident.effects.ssao(), Some(original));
    }
    let mut matching = FrameRenderer::hdr(
        &context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    encode(&context, &mut resident, &mut matching).unwrap();
    assert_ne!(resident.gpu.view_stats(), before);
}

#[test]
fn shadowed_sources_have_exactly_one_ambient_injection_marker() {
    for source in [resources::SHADOWED_WGSL, resources::SHADOWED_TERRAIN_WGSL] {
        let result = crate::ssao::ambient_source(source);
        assert_eq!(result.matches("* screen_ao(world_position)").count(), 1);
        assert!(!result.contains("/* SCREEN_AMBIENT_FACTOR */"));
    }
}
