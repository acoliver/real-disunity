use super::*;
use crate::{GpuTiming, GpuTimingUnavailable, PassId, PassSlot, StageId};

fn assert_usable_durations(frame: &crate::FrameTiming) {
    assert!(frame.passes.iter().all(|pass| {
        pass.duration_ns
            .is_some_and(|ns| ns.is_finite() && ns >= 0.0)
    }));
    assert!(
        frame
            .passes
            .iter()
            .any(|pass| pass.duration_ns.is_some_and(|ns| ns > 0.0))
    );
}

#[test]
fn shadow_metadata_ao_forward_ssr_water_resolve_and_post_have_query_pairs() {
    let host = RetainedHost::new(
        effects_scene(),
        crate::ViewerOptions {
            gpu_timing: true,
            shadows: Some(crate::renderer::retained_tests::shadow_settings()),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 4,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut driver = Driver::default();
    let mut run = host.windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    let Some(frame) = completed_sample(&mut run) else {
        run.shutdown().unwrap();
        return;
    };
    for name in ["sdsm receiver initialize", "sdsm receiver reduce"] {
        assert_eq!(
            frame
                .passes
                .iter()
                .filter(|p| p.pass_id == PassId::Stage(StageId::new(name, 0)))
                .count(),
            1,
            "one timestamp pair for {name}"
        );
    }
    for slot in [
        PassSlot::ShadowCasters(0),
        PassSlot::DepthNormalPrepass,
        PassSlot::Ssao,
        PassSlot::Opaque,
        PassSlot::TransparentWater,
        PassSlot::ReflectionTrace,
    ] {
        let passes: Vec<_> = frame
            .passes
            .iter()
            .filter(|p| p.pass_id == PassId::Geometry(slot))
            .collect();
        assert_eq!(passes.len(), 1, "one timestamp pair for {slot:?}");
        assert!(passes[0].duration_ns.is_some());
    }
    assert!(frame.passes.iter().any(|p| p.pass_id == PassId::Resolve));
    assert!(
        frame
            .passes
            .iter()
            .any(|p| matches!(p.pass_id, PassId::Stage(_)))
    );
    run.shutdown().unwrap();
}

fn effects_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::source();
    let effects = base.effects().unwrap();
    let effects = SceneEffects::new(SceneEffectsParts {
        water: crate::tests::scene(true)
            .effects()
            .unwrap()
            .water()
            .cloned(),
        sky: *effects.sky(),
        sun: *effects.sun(),
        ambient: *effects.ambient(),
        shadows: effects.shadows().clone(),
        camera: *effects.camera(),
        grading: *effects.grading(),
        post: vec![],
        anti_alias: *effects.anti_alias(),
        ssao: Some(SceneSsao {
            depth_footprint: SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Hemisphere,
            filter: SceneSsaoFilter::DepthAwareBox,
            sample_count: 4,
            bias_millimeters: 1,
            radius_millimeters: 100,
            intensity_micro: 1_000_000,
            horizon_angle_millidegrees: 90_000,
        }),
    })
    .unwrap()
    .with_ssr(SceneSsr {
        max_steps: 8,
        ..SceneSsr::default()
    })
    .unwrap();
    base.with_objects_and_effects(
        base.geometry().objects().to_vec(),
        Some(depth_fitted(effects)),
    )
    .unwrap()
}

fn depth_fitted(effects: SceneEffects) -> SceneEffects {
    use worldrunner_render_plan::{SceneDepthAdoption, SceneDepthFitting, SceneDepthSplits};
    effects
        .with_depth_fitting(SceneDepthFitting {
            near_step_mm: 100,
            far_step_mm: 1000,
            minimum_span_mm: 1000,
            minimum_span_policy: worldrunner_render_plan::SceneDepthMinimumSpan::ExtendNear,
            active_maximum_mm: None,
            splits: SceneDepthSplits::Practical {
                lambda_micro: 500_000,
            },
            adoption: SceneDepthAdoption::FrameIndexed {
                latency_frames: 2,
                contraction_frames: 4,
            },
        })
        .unwrap()
}

#[test]
fn requesting_timing_preserves_rendered_bytes() {
    for profile in [
        crate::FrameProfile::Legacy,
        crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
    ] {
        let mut images = Vec::new();
        for requested in [false, true] {
            let mut host = host(crate::renderer::retained_tests::source());
            host.options.gpu_timing = requested;
            host.options.frame_profile = profile;
            let mut driver = Driver::default();
            let events = driver.events.clone();
            let mut run = host.windowless(&mut driver).unwrap();
            images.push(capture(&mut run, &events).1);
            run.shutdown().unwrap();
        }
        assert_eq!(images[0], images[1]);
    }
}

#[test]
fn timing_is_disabled_without_a_request() {
    let host = host(crate::renderer::retained_tests::source());
    assert!(!host.options.gpu_timing);
    assert_eq!(host.gpu_timing(), &GpuTiming::Disabled);
    let mut driver = Driver::default();
    let mut run = host.windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    run.poll().unwrap();
    assert_eq!(run.host().gpu_timing(), &GpuTiming::Disabled);
    run.shutdown().unwrap();
}

#[test]
fn requested_timing_reports_capability_or_completed_frame_without_more_rendering() {
    let mut host = host(crate::renderer::retained_tests::source());
    host.options.gpu_timing = true;
    let mut driver = Driver::default();
    let mut run = host.windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    let Some(frame) = completed_sample(&mut run) else {
        run.shutdown().unwrap();
        return;
    };
    assert_eq!(frame.frame_id, 0);
    assert!(frame.passes.len() <= 1024);
    assert!(frame.passes.iter().all(|pass| pass.duration_ns.is_some()));
    assert!(
        frame
            .passes
            .iter()
            .any(|pass| pass.pass_id == PassId::Geometry(PassSlot::Opaque))
    );
    assert!(
        frame
            .passes
            .iter()
            .any(|pass| pass.pass_id == PassId::Stage(StageId::new("post copy output", 0)))
    );
    assert_resize_retains_sample_and_labels(&mut run);
    run.shutdown().unwrap();
}

fn assert_resize_retains_sample_and_labels(run: &mut WindowlessRetained<'_>) {
    let previous = run.host().gpu_timing().clone();
    run.poll().unwrap();
    assert_eq!(run.host().gpu_timing(), &previous);
    run.resize([48, 48]).unwrap();
    assert_eq!(run.host().gpu_timing(), &previous);
    let GpuTiming::Complete(previous) = previous else {
        unreachable!()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        run.frame().unwrap();
        run.poll().unwrap();
        if let GpuTiming::Complete(next) = run.host().gpu_timing()
            && next.frame_id > previous.frame_id
        {
            assert_usable_durations(next);
            assert_eq!(
                next.passes.iter().map(|p| p.pass_id).collect::<Vec<_>>(),
                previous
                    .passes
                    .iter()
                    .map(|p| p.pass_id)
                    .collect::<Vec<_>>()
            );
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn completed_sample(run: &mut WindowlessRetained<'_>) -> Option<crate::FrameTiming> {
    let (supported, backend) = crate::renderer::timing::adapter_timing(&run.renderer);
    let deadline = Instant::now() + Duration::from_secs(10);
    while matches!(run.host().gpu_timing(), GpuTiming::Pending) {
        run.poll().unwrap();
        assert!(
            Instant::now() < deadline,
            "timestamp mapping did not complete"
        );
        std::thread::yield_now();
    }
    match run.host().gpu_timing() {
        GpuTiming::Complete(frame) => {
            assert!(!supported.is_empty());
            assert_usable_durations(frame);
            Some(frame.clone())
        }
        GpuTiming::Unavailable(GpuTimingUnavailable::MissingFeatures(missing)) => {
            assert!(supported.is_empty(), "selected adapter supports timestamps");
            assert_eq!(*missing, wgpu::Features::TIMESTAMP_QUERY);
            eprintln!("selected adapter lacks timestamps: {missing:?}");
            None
        }
        GpuTiming::Unavailable(GpuTimingUnavailable::InvalidTimestampPair {
            frame_id,
            pass_id,
        }) => {
            assert!(!supported.is_empty());
            assert_eq!(
                backend,
                wgpu::Backend::Metal,
                "invalid counters on a usable backend"
            );
            assert_eq!(*frame_id, 0);
            eprintln!("explicit unusable Metal timestamp pair: frame={frame_id} pass={pass_id:?}");
            None
        }
        other => panic!("expected a completed timing observation, got {other:?}"),
    }
}
