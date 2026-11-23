use super::super::{DirectionalShadowSettings, GpuContext};
use super::{
    context,
    fixture::{scene, settings, view},
};
use crate::draw::PreparedScene;
use crate::shadows::ShadowedTarget;
use crate::{SceneView, offscreen::OffscreenTarget, test_support::SceneTestSession};
use worldrunner_render_gpu::{census::ResourceClass, submission::PreparedFrame};

fn prepare_frame(
    context: &GpuContext,
    renderer: &mut super::super::ShadowedScene,
    scheduler: &mut crate::compositor::FrameRenderer,
    camera: &SceneView,
    target: &OffscreenTarget,
) -> PreparedFrame {
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            encode_frame(context, (renderer, scheduler), camera, target, encoder)
        })
        .unwrap()
}

fn encode_frame(
    context: &GpuContext,
    (renderer, scheduler): (
        &mut super::super::ShadowedScene,
        &mut crate::compositor::FrameRenderer,
    ),
    camera: &SceneView,
    target: &OffscreenTarget,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), super::super::ShadowError> {
    renderer.encode_with(
        context,
        camera,
        (&target.color_attachment(), &target.depth_attachment()),
        encoder,
        scheduler,
    )
}

#[test]
fn replaced_shadow_map_stays_counted_until_prepared_frame_is_cancelled() {
    let _session = SceneTestSession::acquire();
    let context = context();
    let census = context.submission().census().clone();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = super::super::ShadowedScene::new(
        &context,
        PreparedScene::new(&source).unwrap(),
        settings(),
        ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
    )
    .unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let camera = view();
    let target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let old_bytes = census.snapshot().get(ResourceClass::ShadowMap);
    let old_layer_bytes = u64::from(settings().resolution).pow(2) * 4;
    let layers = old_bytes / old_layer_bytes;
    assert!(layers > 0);
    assert_eq!(old_bytes, layers * old_layer_bytes);

    let prepared = prepare_frame(&context, &mut renderer, &mut scheduler, &camera, &target);
    let next_settings = DirectionalShadowSettings {
        resolution: 64,
        ..settings()
    };
    renderer.set_settings(&context, next_settings).unwrap();
    let new_bytes = layers * u64::from(next_settings.resolution).pow(2) * 4;
    assert_eq!(
        census.snapshot().get(ResourceClass::ShadowMap),
        old_bytes + new_bytes,
        "the prepared command buffer still references the replaced shadow map"
    );

    prepared.cancel();
    assert_eq!(census.snapshot().get(ResourceClass::ShadowMap), new_bytes);
}

#[test]
fn replaced_shadow_map_stays_counted_through_submission_completion() {
    let _session = SceneTestSession::acquire();
    let context = context();
    let census = context.submission().census().clone();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = super::super::ShadowedScene::new(
        &context,
        PreparedScene::new(&source).unwrap(),
        settings(),
        ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
    )
    .unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let camera = view();
    let target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let old_bytes = census.snapshot().get(ResourceClass::ShadowMap);
    let old_layer_bytes = u64::from(settings().resolution).pow(2) * 4;
    let layers = old_bytes / old_layer_bytes;
    assert_eq!(old_bytes, layers * old_layer_bytes);
    let prepared = prepare_frame(&context, &mut renderer, &mut scheduler, &camera, &target);
    let next_settings = DirectionalShadowSettings {
        resolution: 64,
        ..settings()
    };
    renderer.set_settings(&context, next_settings).unwrap();
    let new_bytes = layers * u64::from(next_settings.resolution).pow(2) * 4;
    assert_eq!(
        census.snapshot().get(ResourceClass::ShadowMap),
        old_bytes + new_bytes,
        "the prepared frame retains the replaced shadow map"
    );
    let submission = prepared.submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    assert_eq!(census.snapshot().get(ResourceClass::ShadowMap), new_bytes);
}

#[test]
fn shadow_map_replaced_during_encode_retains_the_recorded_generation() {
    let _session = SceneTestSession::acquire();
    let context = context();
    let census = context.submission().census().clone();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = super::super::ShadowedScene::new(
        &context,
        PreparedScene::new(&source).unwrap(),
        settings(),
        ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
    )
    .unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let old_bytes = census.snapshot().get(ResourceClass::ShadowMap);
    let camera = view();
    let target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let next_effects = super::with_cascades(source.effects().unwrap(), vec![4_000, 8_000], 0);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            encode_frame(
                &context,
                (&mut renderer, &mut scheduler),
                &camera,
                &target,
                encoder,
            )?;
            renderer.set_effects(&next_effects)?;
            encode_frame(
                &context,
                (&mut renderer, &mut scheduler),
                &camera,
                &target,
                encoder,
            )
        })
        .unwrap();
    let new_bytes = u64::from(settings().resolution).pow(2) * 2 * 4;
    assert_eq!(
        census.snapshot().get(ResourceClass::ShadowMap),
        old_bytes + new_bytes,
        "both recorded shadow generations remain counted"
    );
    let final_settings = DirectionalShadowSettings {
        resolution: 64,
        ..settings()
    };
    renderer.set_settings(&context, final_settings).unwrap();
    let final_bytes = u64::from(final_settings.resolution).pow(2) * 2 * 4;
    assert_eq!(
        census.snapshot().get(ResourceClass::ShadowMap),
        old_bytes + new_bytes + final_bytes,
        "the frame retains the replacement selected during encode"
    );
    prepared.cancel();
    assert_eq!(census.snapshot().get(ResourceClass::ShadowMap), final_bytes);
}
