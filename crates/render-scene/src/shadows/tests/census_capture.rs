use super::super::{DirectionalShadowSettings, GpuContext, ShadowError, ShadowedScene};
use super::{
    context,
    fixture::{scene, settings, view},
};
use crate::draw::PreparedScene;
use crate::shadows::ShadowedTarget;
use crate::{
    SceneView, Viewport,
    compositor::{
        ConfiguredFrameRenderer, RendererConfiguration,
        terms::{CaptureTerm, TermCapture},
    },
    offscreen::OffscreenTarget,
    test_support::SceneTestSession,
};
use worldrunner_render_gpu::{census::ResourceClass, configuration::AllocationBudget};

#[test]
fn capture_only_frame_retains_replaced_shadow_map() {
    let _session = SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(
        &context,
        PreparedScene::new(&source).unwrap(),
        settings(),
        ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
    )
    .unwrap();
    let camera = view()
        .with_raster_sample(
            Viewport {
                width: 64,
                height: 64,
            },
            [0.0; 2],
        )
        .unwrap();
    let target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let mut scheduler = capture_scheduler(&context, &camera);
    initialize(&context, &mut renderer, &camera, &target, &mut scheduler);
    let census = context.submission().census().clone();
    let old_bytes = u64::from(settings().resolution).pow(2) * 4;
    assert_eq!(census.snapshot().get(ResourceClass::ShadowMap), old_bytes);
    let capture = TermCapture::new(context.device(), [64, 64], CaptureTerm::SunVisibility);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode_term_capture(context.device(), encoder, &capture, &scheduler)?;
            Ok::<_, ShadowError>(())
        })
        .unwrap();
    let next = DirectionalShadowSettings {
        resolution: 64,
        ..settings()
    };
    renderer.set_settings(&context, next).unwrap();
    let new_bytes = u64::from(next.resolution).pow(2) * 4;
    assert_eq!(
        census.snapshot().get(ResourceClass::ShadowMap),
        old_bytes + new_bytes,
        "capture-only commands still reference the replaced shadow map"
    );
    prepared.cancel();
    assert_eq!(census.snapshot().get(ResourceClass::ShadowMap), new_bytes);
}

pub(super) fn capture_scheduler(
    context: &GpuContext,
    camera: &SceneView,
) -> ConfiguredFrameRenderer {
    let viewport = camera.viewport();
    let size = [viewport.width, viewport.height];
    let mut config = RendererConfiguration::new(
        size,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        worldrunner_render_post::ToneMap::Clamp,
    );
    config.additional = RendererConfiguration::capture_term_allocations(size);
    ConfiguredFrameRenderer::new(context, config, AllocationBudget(16 * 1024 * 1024)).unwrap()
}

fn initialize(
    context: &GpuContext,
    renderer: &mut ShadowedScene,
    camera: &SceneView,
    target: &OffscreenTarget,
    scheduler: &mut ConfiguredFrameRenderer,
) {
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let color = target.color_attachment();
            let depth = target.depth_attachment();
            scheduler.encoding().encode_shadowed(
                context,
                camera,
                (&color, &depth),
                encoder,
                renderer,
            )
        })
        .unwrap();
    let submission = prepared.submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
}
