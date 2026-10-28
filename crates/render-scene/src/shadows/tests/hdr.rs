use super::*;
use crate::compositor::FrameRenderer;
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

fn hdr_capture(
    context: &GpuContext,
    source: &RenderScene,
    samples: u32,
    bias: f32,
) -> (Vec<u8>, RgbaImage) {
    let view = view();
    let size = [view.viewport().width, view.viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        samples,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        DirectionalShadowSettings {
            depth_bias: bias,
            ..settings()
        },
        wgpu::TextureFormat::Rgba16Float,
        samples,
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
    let mut output = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encode_with(
            context,
            &view,
            (&color, &depth),
            &mut encoder,
            Some(&mut frame),
        )
        .unwrap();
    frame
        .encode_output(&mut encoder, output.color_view())
        .unwrap();
    output.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    let image = output.read_encoded(context, submission).unwrap();
    let raw = read_hdr(context, &frame, size);
    (raw, image)
}
fn exposed(source: &RenderScene, exposure: u32) -> RenderScene {
    let mut effects = fixture::effects_parts(600_000);
    effects.sun.irradiance_micro = 10_000_000;
    effects.grading.exposure_micro = exposure;
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: source.terrain().cloned(),
        effects: Some(SceneEffects::new(effects).unwrap()),
    })
    .unwrap()
}

#[test]
fn hdr_mesh_and_terrain_shadows_preserve_unexposed_radiance_at_one_and_four_samples() {
    let context = context();
    for samples in [1, 4] {
        for terrain in [false, true] {
            let mut source = scene(&[[0, 0, 1_000]], 600_000, None);
            if terrain {
                source = super::terrain::terrain_scene(&source, false, false);
            }
            let (raw, dim) = hdr_capture(&context, &exposed(&source, 500_000), samples, 0.001);
            let (bright_raw, bright) =
                hdr_capture(&context, &exposed(&source, 4_000_000), samples, 0.001);
            assert_eq!(
                raw, bright_raw,
                "scene exposure belongs exclusively to post"
            );
            assert!(
                raw.chunks_exact(8)
                    .any(|pixel| u16::from_le_bytes([pixel[0], pixel[1]]) > 0x3c00),
                "lit radiance must exceed one before tone mapping"
            );
            assert!(at(&bright, Vec3::new(1.0, 0.0, 0.0)) > at(&dim, Vec3::new(1.0, 0.0, 0.0)));
            let (_, baseline) = hdr_capture(&context, &exposed(&source, 500_000), samples, 0.9);
            assert_shadow(&dim, &baseline, Vec3::new(-0.75, 0.0, 0.0));
            assert_lit(&dim, &baseline, Vec3::new(1.0, 0.0, 0.0));
        }
    }
}

#[test]
fn hdr_profile_rejects_uninstalled_settings_before_gpu_execution() {
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    for samples in [0, 2, 8, 3] {
        assert!(
            matches!(FrameRenderer::hdr(&context, [8, 8], OffscreenTarget::COLOR_FORMAT, samples, ToneMap::Reinhard, None), Err(worldrunner_render_post::PostError::SampleCount(value)) if value == samples)
        );
    }
    assert!(
        FrameRenderer::hdr(
            &context,
            [8, 8],
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Filmic,
            None
        )
        .is_ok()
    );
    let mut effects = fixture::effects_parts(600_000);
    effects.anti_alias = SceneAntiAlias::Taa { history_micro: 0 };
    let effects = SceneEffects::new(effects).unwrap();
    assert!(
        FrameRenderer::hdr(
            &context,
            [8, 8],
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            Some(&effects)
        )
        .is_err()
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn fixed_exposure_mismatch_is_typed_before_preparation() {
    let context = context();
    let source = exposed(&scene(&[], 600_000, None), 500_000);
    let frame = FrameRenderer::hdr(
        &context,
        [8, 8],
        OffscreenTarget::COLOR_FORMAT,
        4,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let original = FrameUniform::new(&view(), source.effects()).unwrap();
    let unexposed = frame.prepare_uniform(original).unwrap();
    assert!((unexposed.exposure - 1.0).abs() < f32::EPSILON);
    assert_eq!(
        frame.prepare_uniform(FrameUniform {
            background_far_plane_meters: 0.0,
            exposure: 2.0,
            ..original
        }),
        Err(worldrunner_render_post::FrameError::FixedExposureMismatch)
    );
}

#[test]
fn ssao_opaque_mesh_and_terrain_produce_shadowed_hdr_output() {
    let context = context();
    for samples in [1, 4] {
        let base =
            super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), false, false);
        let mut parts = fixture::effects_parts(600_000);
        parts.ssao = Some(SceneSsao {
            depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
            kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
            filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
            radius_millimeters: 2000,
            intensity_micro: 1_000_000,
            sample_count: 32,
            bias_millimeters: 10,
            horizon_angle_millidegrees: 90_000,
        });
        let source = RenderScene::new(SceneParts {
            resources: base.resources().clone(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: Some(SceneEffects::new(parts).unwrap()),
        })
        .unwrap();
        let (raw, image) = hdr_capture(&context, &source, samples, 0.001);
        assert!(raw.iter().any(|byte| *byte != 0));
        assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[0] > 0));
    }
}

#[test]
fn ssr_profile_renders_nonempty_shadowed_mesh_and_terrain_at_1x_and_4x() {
    let context = context();
    for samples in [1, 4] {
        let base =
            super::terrain::terrain_scene(&scene(&[[0, 0, 1000]], 600_000, None), false, false);
        let effects = base
            .effects()
            .unwrap()
            .clone()
            .with_ssr(worldrunner_render_plan::SceneSsr {
                enabled: true,
                max_steps: 64,
                thickness_millimeters: 100,
                roughness_cutoff_micro: 600_000,
                resolution_scale_micro: 500_000,
                ..Default::default()
            })
            .unwrap();
        let source = RenderScene::new(SceneParts {
            resources: base.resources().clone(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: Some(effects),
        })
        .unwrap();
        let (raw, image) = hdr_capture(&context, &source, samples, 0.001);
        assert!(raw.iter().any(|byte| *byte != 0));
        assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[0] > 0));
    }
}

#[test]
fn hdr_post_effects_run_after_resolve_with_msaa_and_deterministic_capture() {
    use worldrunner_render_plan::{SceneAdaptiveExposure, SceneBloom, SceneHdrPost};
    let context = context();
    let source = exposed(&scene(&[[0, 0, 1_000]], 600_000, None), 1_000_000);
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_hdr_post(SceneHdrPost {
            tone_map: None,
            grading: None,
            adaptive_exposure: Some(SceneAdaptiveExposure {
                metering: None,
                log_min_milli: -12_000,
                log_max_milli: 12_000,
                low_percentile_micro: 10_000,
                high_percentile_micro: 990_000,
                key_micro: 180_000,
                min_exposure_micro: 10_000,
                max_exposure_micro: 64_000_000,
                speed_up_milli: 3_000,
                speed_down_milli: 1_000,
                fixed_delta_microseconds: Some(250_000),
                fixed_exposure_micro: None,
            }),
            bloom: Some(SceneBloom {
                threshold_micro: 500_000,
                soft_knee_micro: 500_000,
                intensity_micro: 500_000,
                levels: 4,
                recipe: None,
            }),
            fxaa_subpixel_micro: Some(750_000),
            fxaa_quality: None,
        })
        .unwrap();
    let filtered = RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap();
    for samples in [1, 4] {
        let (plain_hdr, plain) = hdr_capture(&context, &source, samples, 0.001);
        let (filtered_hdr, image) = hdr_capture(&context, &filtered, samples, 0.001);
        assert_eq!(plain_hdr, filtered_hdr, "post never changes scene radiance");
        assert_ne!(
            plain.pixels, image.pixels,
            "effects must contribute at each sample count"
        );
        let (_, repeated) = hdr_capture(&context, &filtered, samples, 0.001);
        assert_eq!(image.pixels, repeated.pixels);
    }
}

fn read_hdr(context: &GpuContext, frame: &FrameRenderer, size: [u32; 2]) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        2_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        frame.targets().texture(),
    )
    .unwrap()
}

#[test]
fn authored_display_grade_reaches_the_existing_hdr_executor() {
    use worldrunner_render_plan::{SceneDisplayGrading, SceneHdrPost, SceneToneMap};
    let context = context();
    let source = exposed(&scene(&[[0, 0, 1_000]], 600_000, None), 1_000_000);
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_hdr_post(SceneHdrPost {
            tone_map: Some(SceneToneMap::AcesFitted),
            grading: Some(SceneDisplayGrading {
                linear_scale_micro: 2_000_000,
                display_power_micro: [1_250_000, 1_000_000, 750_000],
                vignette: None,
            }),
            adaptive_exposure: None,
            bloom: None,
            fxaa_subpixel_micro: None,
            fxaa_quality: None,
        })
        .unwrap();
    let settings = crate::post_settings(&effects).unwrap();
    assert_eq!(settings.tone_map, Some(ToneMap::AcesFitted));
    assert_eq!(settings.grading.linear_scale.to_bits(), 2.0_f32.to_bits());
    assert_eq!(
        settings.grading.display_power.map(f32::to_bits),
        [1.25_f32, 1.0, 0.75].map(f32::to_bits)
    );
    let graded = RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap();
    let (plain_hdr, plain) = hdr_capture(&context, &source, 1, 0.001);
    let (graded_hdr, image) = hdr_capture(&context, &graded, 1, 0.001);
    assert_eq!(plain_hdr, graded_hdr);
    assert_ne!(plain.pixels, image.pixels);
}
