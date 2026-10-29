use super::retained_tests::{shadow_settings, source};
use super::*;
use worldrunner_render_plan::{SceneDepthAdoption, SceneDepthFitting, SceneDepthSplits};
use worldrunner_render_scene::shadows::sdsm::{AdoptedFit, AdoptionPolicy};

fn finish_timed_output(context: &GpuContext, presentation: &Presentation) {
    let output = worldrunner_render_post::color_texture(
        context.device(),
        "depth timing output",
        [64, 64],
        crate::presentation::COLOR,
    );
    let view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let submission = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            presentation.scheduler.encode_output(encoder, &view)
        })
        .unwrap()
        .submit()
        .unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
}

#[test]
fn depth_fitting_reduction_passes_share_interactive_and_capture_samples() {
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let supported = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
    let context = pollster::block_on(GpuContext::from_adapter_with_timing(
        adapter,
        wgpu::Features::empty(),
        worldrunner_render_post::TimingMode::Optional,
    ))
    .unwrap();
    assert_eq!(
        context.device().features() & wgpu::Features::TIMESTAMP_QUERY,
        supported
    );
    if supported.is_empty() {
        return;
    }
    let (mut resident, mut presentation) = fixture(&context);
    for (frame_id, capture) in [(0, false), (1, true)] {
        render_frame(
            &context,
            &mut resident,
            &mut presentation,
            (Some(frame_id), capture),
        );
        finish_timed_output(&context, &presentation);
        let samples = presentation.scheduler.take_timings().unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].frame_id, frame_id);
        for name in [
            "sdsm receiver initialize",
            "sdsm receiver reduce",
            "post copy output",
        ] {
            assert_eq!(
                samples[0]
                    .passes
                    .iter()
                    .filter(
                        |pass| pass.pass_id == crate::PassId::Stage(crate::StageId::new(name, 0))
                    )
                    .count(),
                1
            );
        }
        assert!(
            samples[0]
                .passes
                .iter()
                .any(|pass| pass.pass_id == crate::PassId::Geometry(crate::PassSlot::Opaque))
        );
    }
}

#[test]
fn retained_sdsm_selects_delayed_fit_and_same_frame_capture_then_invalidates_resize() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let (mut resident, mut presentation) = fixture(&context);
    assert_eq!(
        render(&context, &mut resident, &mut presentation, false),
        None
    );
    assert_eq!(
        render(&context, &mut resident, &mut presentation, false),
        None
    );
    let fit = render(&context, &mut resident, &mut presentation, false).unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 2));
    let fit = render(&context, &mut resident, &mut presentation, true).unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (3, 3));
    let Resident::Shadowed(shadowed) = &resident else {
        panic!("shadow resident");
    };
    assert!(matches!(
        shadowed.depth_fitting_settings().unwrap().policy,
        AdoptionPolicy::QuantizedPerBound {
            latency_frames: 2,
            ..
        }
    ));
    presentation
        .resize(
            &context,
            Viewport {
                width: 32,
                height: 32,
            },
            Viewport {
                width: 32,
                height: 32,
            },
        )
        .unwrap();
    assert_eq!(
        render(&context, &mut resident, &mut presentation, false),
        None
    );
    let fit = render(&context, &mut resident, &mut presentation, true).unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (5, 5));
    let Resident::Shadowed(shadowed) = &mut resident else {
        panic!("shadow resident");
    };
    shadowed.set_depth_fitting(&context, None).unwrap();
    assert!(shadowed.last_depth_fit().is_none());
}

fn render(
    context: &GpuContext,
    resident: &mut Resident,
    presentation: &mut Presentation,
    capture: bool,
) -> Option<AdoptedFit> {
    render_frame(context, resident, presentation, (None, capture))
}

fn render_frame(
    context: &GpuContext,
    resident: &mut Resident,
    presentation: &mut Presentation,
    frame: (Option<u64>, bool),
) -> Option<AdoptedFit> {
    context.device().poll(wgpu::PollType::Poll).unwrap();
    let options = crate::tests::options();
    let size = presentation.scheduler.configuration().frame.size;
    let view = crate::OrbitCamera::new(options.pose, options.orbit)
        .unwrap()
        .view(
            options.camera,
            Viewport {
                width: size[0],
                height: size[1],
            },
        )
        .unwrap();
    assert!(
        resident
            .render_depth_fitted(
                context,
                &view,
                &presentation.targets,
                presentation.scheduler.encoding(),
                frame
            )
            .unwrap()
    );
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let Resident::Shadowed(shadowed) = resident else {
        panic!("shadow resident");
    };
    shadowed.last_depth_fit()
}

fn fixture(context: &GpuContext) -> (Resident, Presentation) {
    let scene = fitted_scene();
    assert!(Resident::new(context, &scene, None, crate::FrameProfile::Legacy).is_err());
    let resident = Resident::new(
        context,
        &scene,
        Some(shadow_settings()),
        crate::FrameProfile::Legacy,
    )
    .unwrap();
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let presentation = Presentation::new(context, wgpu::TextureFormat::Rgba8UnormSrgb, size);
    (resident, presentation)
}

pub(super) fn fitted_scene() -> worldrunner_render_plan::RenderScene {
    let base = source();
    let settings = SceneDepthFitting {
        near_step_mm: 100,
        far_step_mm: 1000,
        minimum_span_mm: 1000,
        minimum_span_policy: worldrunner_render_plan::SceneDepthMinimumSpan::ExtendNear,
        active_maximum_mm: None,
        splits: SceneDepthSplits::OverlapRecurrence {
            multiplier_micro: 2_000_000,
            overlap_micro: 200_000,
        },
        adoption: SceneDepthAdoption::QuantizedPerBound {
            latency_frames: 2,
            contraction_delay_ms: 100,
        },
    };
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_depth_fitting(settings)
        .unwrap();
    base.with_objects_and_effects(base.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

#[test]
fn retained_settings_enable_fitting_without_resetting_host_frame_identity() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let scene = fitted_scene();
    let base = source();
    let mut resident = Resident::new(
        &context,
        &base,
        Some(shadow_settings()),
        crate::FrameProfile::Legacy,
    )
    .unwrap();
    let mut presentation = Presentation::new(
        &context,
        crate::presentation::COLOR,
        Viewport {
            width: 64,
            height: 64,
        },
    );
    super::retained::configure_effects(
        &context,
        &mut resident,
        &mut presentation,
        scene.effects().unwrap(),
    )
    .unwrap();
    let fit = render_frame(&context, &mut resident, &mut presentation, (Some(42), true)).unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (42, 42));
    super::retained::configure_effects(
        &context,
        &mut resident,
        &mut presentation,
        scene.effects().unwrap(),
    )
    .unwrap();
    let fit = render_frame(&context, &mut resident, &mut presentation, (Some(43), true)).unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (43, 43));
}
