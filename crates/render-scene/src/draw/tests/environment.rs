use super::*;
use crate::frame::FrameError;
use worldrunner_render_plan::{ScenePostStage, SceneSsao};

#[test]
fn absent_environment_selects_unlit_frame_encoding() {
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let frame = FrameUniform::new(&view, None).unwrap();
    assert!(frame.unlit);
    assert_eq!(frame.sun_irradiance, glam::Vec3::ZERO);
    assert_eq!(&frame.to_bytes()[148..152], &1.0_f32.to_le_bytes());
}

#[test]
fn stock_frames_reject_each_requested_unimplemented_effect() {
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let mut parts = effect_parts();
    parts.post = vec![ScenePostStage::Vignette { strength_micro: 0 }];
    let effects = SceneEffects::new(parts).unwrap();
    assert_eq!(
        FrameUniform::new(&view, Some(&effects)),
        Err(FrameError::UnsupportedEffect("post-processing"))
    );
}

/// The stock frame uniform carries no resolve: TAA, like FXAA, belongs to the `FrameRenderer`
/// that owns the targets and the history, which refuses a mismatched scene in `validate_post`.
#[test]
fn stock_frames_accept_authored_taa_because_the_renderer_owns_the_resolve() {
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let mut parts = effect_parts();
    parts.anti_alias = SceneAntiAlias::taa(0);
    let effects = SceneEffects::new(parts).unwrap();
    assert_eq!(
        FrameUniform::new(&view, Some(&effects)).unwrap().temporal,
        None
    );
}

#[test]
fn gpu_scene_without_environment_renders_unlit_albedo() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 1);
    let unlit = RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let image = crate::offscreen::capture_scene(&context, &unlit, &view).unwrap();
    let side = usize::try_from(TARGET).unwrap();
    let center = (side / 2 * side + side / 2) * 4;
    assert_eq!(&image.pixels[center..center + 4], &[255, 255, 255, 255]);
}

#[test]
fn gpu_upload_rejects_requested_ssao_before_uploading_resources() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 1);
    let mut parts = effect_parts();
    parts.ssao = Some(SceneSsao {
        depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
        sample_count: 16,
        bias_millimeters: 10,
        radius_millimeters: 500,
        intensity_micro: 0,
        horizon_angle_millidegrees: 30_000,
    });
    let source = RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(SceneEffects::new(parts).unwrap()),
    })
    .unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    assert!(matches!(
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines),
        Err(DrawError::Frame(FrameError::UnsupportedEffect("SSAO")))
    ));
}

#[test]
fn stock_frame_lighting_accepts_supported_ssao_settings() {
    let mut parts = effect_parts();
    parts.ssao = Some(SceneSsao {
        depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 500,
        intensity_micro: 1_000_000,
        horizon_angle_millidegrees: 90_000,
        sample_count: 16,
        bias_millimeters: 10,
    });
    let effects = SceneEffects::new(parts).unwrap();
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    assert!(FrameUniform::new(&view, Some(&effects)).is_ok());
}
