use super::super::{DirectionalShadowSettings, GpuContext, ShadowError, ShadowedScene};
use super::{
    context,
    fixture::{scene, settings, view},
};
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
    assert_capture_retention(false);
}

#[test]
fn configured_capture_only_frame_retains_replaced_shadow_map() {
    assert_capture_retention(true);
}

fn assert_capture_retention(configured: bool) {
    let _session = SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
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
    let mut scheduler = configured.then(|| capture_scheduler(&context, &camera));
    initialize(
        &context,
        &mut renderer,
        &camera,
        &target,
        scheduler.as_mut(),
    );
    let census = context.submission().census().clone();
    let old_bytes = u64::from(settings().resolution).pow(2) * 4;
    assert_eq!(census.snapshot().get(ResourceClass::ShadowMap), old_bytes);
    let capture = TermCapture::new(context.device(), [64, 64], CaptureTerm::SunVisibility);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            if let Some(scheduler) = scheduler.as_ref() {
                renderer.encode_term_capture_with(
                    context.device(),
                    encoder,
                    &capture,
                    scheduler,
                )?;
            } else {
                renderer.encode_term_capture(context.device(), encoder, &capture)?;
            }
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
    let mut config = RendererConfiguration::legacy(size, wgpu::TextureFormat::Rgba8UnormSrgb);
    config.additional = RendererConfiguration::capture_term_allocations(size);
    ConfiguredFrameRenderer::new(context, config, AllocationBudget(16 * 1024 * 1024)).unwrap()
}

fn initialize(
    context: &GpuContext,
    renderer: &mut ShadowedScene,
    camera: &SceneView,
    target: &OffscreenTarget,
    scheduler: Option<&mut ConfiguredFrameRenderer>,
) {
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let color = target.color_attachment();
            let depth = target.depth_attachment();
            if let Some(scheduler) = scheduler {
                scheduler.encoding().encode_shadowed(
                    context,
                    camera,
                    (&color, &depth),
                    encoder,
                    renderer,
                )
            } else {
                renderer.encode(context, camera, &color, &depth, encoder)
            }
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
