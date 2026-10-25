use super::*;
use crate::shadows::sdsm::AdoptionState;

fn interval_input(range: RangeFit) -> crate::shadows::sdsm::CaptureInput<()> {
    crate::shadows::sdsm::CaptureInput {
        frame: (),
        source_frame: 0,
        clip_meters: [0.5, 20.0],
        range,
    }
}

fn interval_policy(step: f32, span: f32) -> RangeFit {
    RangeFit {
        near_step: step,
        far_step: step,
        minimum_span: span,
        split_lambda: 0.5,
    }
}
fn capture_state(range: RangeFit) -> AdoptionState {
    AdoptionState::new(SdsmSettings {
        policy: AdoptionPolicy::SameFrameCapture,
        range,
    })
    .unwrap()
}

#[test]
fn gpu_sdsm_capture_interval_matches_cascade_construction() {
    use crate::{frame::metadata::FrameProduct, shadows::sdsm::DepthReducer};
    use worldrunner_render_gpu::submission::capture::CaptureWrites;
    let context = context();
    let source = super::super::super::cascades::source_with(
        &scene(&[], 600_000, None),
        SceneShadowCascades::new(vec![1_000, 2_000, 3_000], 100_000).unwrap(),
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let metadata =
        crate::shadows::sdsm::gpu_tests::source(&context, [9.75, 10.0, 9.75, 10.0], true);
    let reducer = DepthReducer::new(
        context.device(),
        (FrameProduct::linear_depth(0), &metadata.0),
        (FrameProduct::normals(), &metadata.1),
    )
    .unwrap();
    for (step, span) in [(1.0, 1.5), (0.1, 1.37)] {
        let range = interval_policy(step, span);
        let state = capture_state(range);
        let key = reducer.write_key();
        let prefix = reducer
            .begin_capture(
                &context,
                (CaptureWrites::new([key], [], []).unwrap(), key),
                interval_input(range),
                state.ticket(0),
                |_, _, ()| Ok::<_, crate::shadows::sdsm::SdsmError>(()),
            )
            .unwrap();
        let suffix = prefix
            .submit()
            .unwrap()
            .resume(context.device(), |_, _, (), fit| {
                assert_eq!(
                    fit.depth,
                    DepthRange::Receivers {
                        near: 10.0 - span,
                        far: 10.0
                    }
                );
                assert_rendered_interval(&renderer, &source, fit, range);
                Ok::<_, crate::shadows::sdsm::SdsmError>(())
            })
            .unwrap();
        suffix
            .submit()
            .unwrap()
            .finish(context.device(), |_, _| {
                Ok::<_, crate::shadows::sdsm::SdsmError>(())
            })
            .unwrap();
    }
}

#[test]
fn gpu_sdsm_interactive_reuses_cold_and_invalidated_pages() {
    let context = context();
    let source =
        super::super::super::terrain::terrain_scene(&scene(&[], 600_000, None), false, false);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    enable_pages(&context, &mut renderer, 1);
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::Interactive {
                    latency_frames: 1,
                    contraction_frames: 0,
                },
                range: policy(),
            }),
        )
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for generation in 0..2 {
        if generation == 1 {
            renderer.gpu.invalidate_terrain_pages([0.0; 2], [1.0; 2]);
        }
        for frame in 0..6 {
            let serial = context.submission().submitted_serial();
            renderer
                .render(
                    &context,
                    &view(),
                    &target.color_attachment(),
                    &target.depth_attachment(),
                )
                .unwrap();
            assert!(context.submission().submitted_serial() > serial);
            context
                .device()
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            let pixels = target.read_rgba8(&context).unwrap();
            assert!(pixels.pixels.chunks_exact(4).any(|p| p[..3] != [0, 0, 0]));
            assert_eq!(renderer.gpu.terrain_pages_ready(), frame >= 4);
        }
        let fit = renderer
            .render_depth_fitted(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                generation * 7 + 6,
            )
            .unwrap()
            .unwrap();
        assert!(matches!(fit.depth, DepthRange::Receivers { .. }));
    }
}

#[test]
fn gpu_sdsm_abandoned_prefix_does_not_publish_pages() {
    let context = context();
    let source =
        super::super::super::terrain::terrain_scene(&scene(&[], 600_000, None), false, false);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    enable_pages(&context, &mut renderer, 5);
    let metadata = crate::scene_metadata::SceneMetadata::new(
        context.device(),
        [96, 96],
        crate::ssr::neutral_ao(),
    )
    .unwrap();
    let frame = FrameUniform::new(&view(), Some(&renderer.effects)).unwrap();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.scheduler.encode_depth_prefix(
                context.device(),
                encoder,
                (&mut renderer.gpu, &renderer.pipelines),
                frame,
                &metadata,
            )
        })
        .unwrap();
    assert!(!renderer.gpu.terrain_pages_ready());
    drop(prepared);
    assert!(!renderer.gpu.terrain_pages_ready());
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::Interactive {
                    latency_frames: 1,
                    contraction_frames: 0,
                },
                range: policy(),
            }),
        )
        .unwrap();
    renderer
        .render(
            &context,
            &view(),
            &target.color_attachment(),
            &target.depth_attachment(),
        )
        .unwrap();
    assert!(renderer.gpu.terrain_pages_ready());
}

#[test]
fn gpu_sdsm_adopted_interval_is_not_fitted_twice() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    for (step, span) in [(1.0, 1.5), (0.1, 1.37)] {
        for adoption in [
            AdoptionPolicy::SameFrameCapture,
            AdoptionPolicy::Interactive {
                latency_frames: 0,
                contraction_frames: 0,
            },
        ] {
            let range = RangeFit {
                near_step: step,
                far_step: step,
                minimum_span: span,
                split_lambda: 0.5,
            };
            let mut state = AdoptionState::new(SdsmSettings {
                policy: adoption,
                range,
            })
            .unwrap();
            state
                .complete(
                    &state.ticket(0),
                    DepthRange::Receivers {
                        near: 9.75,
                        far: 10.0,
                    },
                )
                .unwrap();
            let fit = state.adopt(0, [0.5, 20.0]).unwrap().unwrap();
            assert_rendered_interval(&renderer, &source, fit, range);
        }
    }
}

fn assert_rendered_interval(
    renderer: &ShadowedScene,
    source: &RenderScene,
    fit: crate::shadows::sdsm::AdoptedFit,
    range: RangeFit,
) {
    let cascades = ShadowedScene::prepare_cascades(
        (&renderer.gpu, source.effects().unwrap(), settings()),
        &view(),
        Vec3::new(0.6, 0.0, 0.8),
        Some((fit.depth, range)),
    )
    .unwrap();
    let DepthRange::Receivers { near, far } = fit.depth else {
        panic!("receivers expected")
    };
    assert_eq!(cascades.near.to_bits(), near.to_bits());
    let count = cascades.matrices.len();
    assert_eq!(cascades.splits[count - 1].to_bits(), far.to_bits());
    assert_eq!(
        cascades.splits[..count],
        range
            .splits(fit.depth, u16::try_from(count).unwrap())
            .unwrap()
    );
}
