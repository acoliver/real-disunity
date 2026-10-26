mod gaps;
mod history;
use super::*;
use crate::shadows::cascades::CascadeViews;
use worldrunner_render_shadow::{DepthRange, RangeFit};
fn policy() -> RangeFit {
    RangeFit {
        near_step: 0.1,
        far_step: 1.0,
        minimum_span: 1.0,
        split_lambda: 0.5,
        split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
    }
}
#[test]
fn depth_fit_keeps_offscreen_casters_and_rescales_bias_with_extent() {
    let context = context();
    let source = scene(&[[1_500, 0, 2_000]], 600_000, None);
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut fits = Vec::new();
    for far in [10.0, 18.0] {
        let mut fit = CascadeViews::new(
            &renderer.gpu,
            &view(),
            source.effects().unwrap(),
            settings(),
            Vec3::new(0.6, 0.0, 0.8),
        )
        .unwrap();
        fit.refit_depth(
            &renderer.gpu,
            &view(),
            settings(),
            Vec3::new(0.6, 0.0, 0.8),
            (DepthRange::Receivers { near: 1.0, far }, policy()),
        )
        .unwrap();
        assert!(fit.camera_splits);
        let matrix = fit.matrices[0];
        let rows = matrix.transpose();
        let texel_width = 2.0 / (rows.x_axis.truncate().length() * 512.0);
        let bias_texels = fit.depth_bias[0] / rows.z_axis.truncate().length() / texel_width;
        let expected = settings().depth_bias * settings().depth_half_extent_meters * 512.0
            / settings().half_extent_meters;
        assert!((bias_texels - expected).abs() < 1e-5);
        fits.push(matrix);
    }
    assert_ne!(fits[0], fits[1]);
}
#[test]
fn empty_depth_has_explicit_zero_receiver_uniform() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut fit = CascadeViews::new(
        &renderer.gpu,
        &view(),
        source.effects().unwrap(),
        settings(),
        Vec3::new(0.6, 0.0, 0.8),
    )
    .unwrap();
    fit.refit_depth(
        &renderer.gpu,
        &view(),
        settings(),
        Vec3::new(0.6, 0.0, 0.8),
        (DepthRange::EmptyDepth, policy()),
    )
    .unwrap();
    let empty = resources::uniform_bytes(&fit, settings());
    assert_eq!(&empty[272..276], &0.0_f32.to_le_bytes());
    assert!(fit.no_receivers);
}

#[test]
fn gpu_depth_fitted_shadow_retains_camera_invisible_caster() {
    let context = context();
    let source = scene(&[[1_500, 0, 2_000]], 600_000, None);
    let view = SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -4_000, 1_500],
            target_millimeters: [0, 0, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 30_000,
            ..*source.effects().unwrap().camera()
        },
        view().viewport(),
    )
    .unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let caster = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let fitted = crate::shadows::sdsm::AdoptedFit {
        source_frame: 1,
        adopted_frame: 1,
        depth: DepthRange::Receivers {
            near: 0.1,
            far: 10.0,
        },
    };
    let mut pixels = Vec::new();
    for visible in [true, false] {
        renderer
            .scene_mut()
            .set_instance_visible(caster, visible)
            .unwrap();
        let frame = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                renderer.encode_depth_fitted(
                    &context,
                    &view,
                    (&target.color_attachment(), &target.depth_attachment()),
                    encoder,
                    (fitted, policy()),
                )
            })
            .unwrap();
        frame.submit().unwrap();
        pixels.push(target.read_rgba8(&context).unwrap());
        if visible {
            assert_eq!(renderer.view_stats().culled, 1);
            assert_eq!(renderer.shadow_view_stats().drawn_per_lod, vec![2]);
        }
    }
    assert!(
        i16::from(at_view(&pixels[1], Vec3::ZERO, &view))
            - i16::from(at_view(&pixels[0], Vec3::ZERO, &view))
            > 40
    );
}
#[test]
fn gpu_stock_sdsm_capture_and_interactive_latency_render_the_adopted_fit() {
    use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let view = view();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut outputs = Vec::new();
    for adoption in [
        AdoptionPolicy::SameFrameCapture,
        AdoptionPolicy::Interactive {
            latency_frames: 2,
            contraction_frames: 0,
        },
    ] {
        renderer
            .set_depth_fitting(
                &context,
                Some(SdsmSettings {
                    policy: adoption,
                    range: policy(),
                }),
            )
            .unwrap();
        let fit = adopted_stock_fit(&mut renderer, &context, &view, &target, adoption);
        assert_eq!(fit.source_frame, 10);
        assert_eq!(
            fit.adopted_frame,
            if adoption == AdoptionPolicy::SameFrameCapture {
                10
            } else {
                12
            }
        );
        assert!(
            matches!(fit.depth, DepthRange::Receivers { near, far } if near > 0.1 && far < 100.0)
        );
        let actual = target.read_rgba8(&context).unwrap();
        assert_fitted_pixels(&mut renderer, &context, &view, &mut target, fit, &actual);
        outputs.push(actual);
    }
    assert_eq!(outputs[0].pixels, outputs[1].pixels);
    assert_ne!(
        outputs[0].pixels,
        capture_scene_shadowed(&context, &source, &view, settings())
            .unwrap()
            .pixels
    );
    let baseline = capture_scene(&context, &source, &view).unwrap();
    assert_shadow(&outputs[0], &baseline, Vec3::new(-0.75, 0.0, 0.0));
    renderer.set_depth_fitting(&context, None).unwrap();
    render_target(&mut renderer, &context, &view, &target).unwrap();
    let disabled = target.read_rgba8(&context).unwrap();
    assert_eq!(
        disabled.pixels,
        capture_scene_shadowed(&context, &source, &view, settings())
            .unwrap()
            .pixels
    );
}

fn adopted_stock_fit(
    renderer: &mut ShadowedScene,
    context: &GpuContext,
    view: &SceneView,
    target: &OffscreenTarget,
    adoption: crate::shadows::sdsm::AdoptionPolicy,
) -> crate::shadows::sdsm::AdoptedFit {
    use crate::shadows::sdsm::AdoptionPolicy;
    let first = renderer
        .render_depth_fitted(
            context,
            view,
            (&target.color_attachment(), &target.depth_attachment()),
            10,
        )
        .unwrap();
    if adoption == AdoptionPolicy::SameFrameCapture {
        assert_eq!(first.unwrap().source_frame, 10);
        first.unwrap()
    } else {
        assert!(first.is_none());
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert!(
            renderer
                .render_depth_fitted(
                    context,
                    view,
                    (&target.color_attachment(), &target.depth_attachment()),
                    11
                )
                .unwrap()
                .is_none()
        );
        renderer
            .render_depth_fitted(
                context,
                view,
                (&target.color_attachment(), &target.depth_attachment()),
                12,
            )
            .unwrap()
            .unwrap()
    }
}

fn assert_fitted_pixels(
    renderer: &mut ShadowedScene,
    context: &GpuContext,
    view: &SceneView,
    target: &mut OffscreenTarget,
    fit: crate::shadows::sdsm::AdoptedFit,
    actual: &RgbaImage,
) {
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode_depth_fitted(
                context,
                view,
                (&target.color_attachment(), &target.depth_attachment()),
                encoder,
                (fit, policy()),
            )
        })
        .unwrap();
    prepared.submit().unwrap();
    assert_eq!(actual.pixels, target.read_rgba8(context).unwrap().pixels);
}

#[test]
fn gpu_stock_sdsm_capture_reuses_hdr_ssao_and_reflection_metadata() {
    use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
    let context = context();
    let (mut frame, mut renderer) = screen_renderers(&context);
    for adoption in [
        AdoptionPolicy::SameFrameCapture,
        AdoptionPolicy::Interactive {
            latency_frames: 2,
            contraction_frames: 0,
        },
    ] {
        renderer
            .set_depth_fitting(
                &context,
                Some(SdsmSettings {
                    policy: adoption,
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
        let source_before = frame.depth_statistics_source().unwrap().unwrap();
        crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
        let fit = renderer
            .render_depth_fitted_with(&context, &view(), (&color, &depth), 3, &mut frame)
            .unwrap();
        if adoption == AdoptionPolicy::SameFrameCapture {
            assert_eq!(fit.unwrap().source_frame, 3);
        }
        crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
            for slot in [
                worldrunner_render_post::PassSlot::DepthNormalPrepass,
                worldrunner_render_post::PassSlot::DepthPyramid(0),
            ] {
                assert_eq!(
                    passes.iter().filter(|&&pass| pass == slot).count(),
                    1,
                    "duplicate producer: {slot:?}"
                );
            }
        });
        let source_after = frame.depth_statistics_source().unwrap().unwrap();
        assert_eq!(source_before.0.texture(), source_after.0.texture());
        assert_eq!(source_before.1.texture(), source_after.1.texture());
    }
}

fn screen_renderers(context: &GpuContext) -> (crate::compositor::FrameRenderer, ShadowedScene) {
    let source = screen_source();
    let size = [view().viewport().width, view().viewport().height];
    let frame = crate::compositor::FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        worldrunner_render_post::ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let renderer = ShadowedScene::with_target(
        context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    (frame, renderer)
}

fn screen_source() -> RenderScene {
    let base = scene(&[[0, 0, 1000]], 600_000, None);
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(SceneSsao {
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: 1_000_000,
        sample_count: 16,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    });
    parts.water = Some(SceneWaterBody {
        material: worldrunner_render_plan::SceneWaterSettings::STOCK,
        surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: -100,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
        east_min_millimeters: -3_000,
        north_min_millimeters: -3_000,
        east_extent_millimeters: 6_000,
        north_extent_millimeters: 6_000,
    });
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_ssr(SceneSsr {
            enabled: true,
            max_steps: 32,
            thickness_millimeters: 100,
            roughness_cutoff_micro: 600_000,
            resolution_scale_micro: 500_000,
        })
        .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}
