use super::*;
use crate::shadows::sdsm::{AdoptedFit, AdoptionPolicy, SdsmSettings};

#[test]
fn gpu_capture_preserves_interactive_contraction_history_and_immediate_expansion() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::QuantizedPerBound {
                    latency_frames: 1,
                    contraction_delay: std::time::Duration::from_hours(1),
                },
                range: RangeFit {
                    near_step: 0.01,
                    far_step: 0.01,
                    minimum_span: 0.01,
                    minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
                    active_maximum: None,
                    ..policy()
                },
            }),
        )
        .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let mut render = |frame, capture, scale| {
        scale_receiver(&mut renderer, scale);
        let fit = renderer
            .render_depth_fitted_host(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                (Some(frame), capture),
                &mut scheduler,
            )
            .unwrap();
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        fit
    };
    assert!(render(0, false, 1_000_000).is_none());
    let initial = render(1, false, 1_000_000).unwrap();
    let captured = render(2, true, 100_000).unwrap();
    assert_eq!((captured.source_frame, captured.adopted_frame), (2, 2));
    assert_contracts(initial, captured);
    render(3, false, 100_000);
    let contracted = render(4, false, 100_000).unwrap();
    assert_eq!(contracted.source_frame, 3);
    assert_eq!(contracted.depth, initial.depth);
    render(5, false, 2_000_000);
    let expanded = render(6, false, 2_000_000).unwrap();
    assert_eq!(expanded.source_frame, 5);
    assert_contracts(expanded, initial);
}

#[test]
fn gpu_fixed_clock_contraction_ignores_host_delay_and_resets_on_rewind() {
    use std::time::Duration;
    let context = context();
    let source = scene(&[], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::QuantizedPerBound {
                    latency_frames: 1,
                    contraction_delay: Duration::from_millis(20),
                },
                range: RangeFit {
                    near_step: 0.01,
                    far_step: 0.01,
                    minimum_span: 0.01,
                    minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
                    active_maximum: None,
                    ..policy()
                },
            }),
        )
        .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let mut render = |frame, millis, scale| {
        scale_receiver(&mut renderer, scale);
        scheduler
            .set_frame_time_duration(Duration::from_millis(millis))
            .unwrap();
        let fit = scheduler
            .encoding()
            .render_depth_fitted(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                (Some(frame), false),
                &mut renderer,
            )
            .unwrap();
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        fit
    };
    assert!(render(0, 100, 1_000_000).is_none());
    let initial = render(1, 100, 1_000_000).unwrap();
    render(2, 100, 100_000);
    assert_eq!(render(3, 100, 100_000).unwrap().depth, initial.depth);
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(render(4, 100, 100_000).unwrap().depth, initial.depth);
    let contracted = render(5, 120, 100_000).unwrap();
    assert_contracts(initial, contracted);
    assert!(render(6, 50, 100_000).is_none());
    assert_eq!(render(7, 50, 100_000).unwrap().depth, contracted.depth);
}

fn scale_receiver(renderer: &mut ShadowedScene, scale_millionths: u64) {
    let receiver = renderer.scene_mut().instance_handles().next().unwrap();
    let original = *renderer
        .scene_mut()
        .instance(receiver)
        .unwrap()
        .0
        .transform();
    renderer
        .scene_mut()
        .set_instance_transform(
            receiver,
            SceneTransform {
                scale_millionths,
                ..original
            },
        )
        .unwrap();
}

fn assert_contracts(outer: AdoptedFit, inner: AdoptedFit) {
    let DepthRange::Receivers { near, far } = outer.depth else {
        panic!("outer range has no receivers");
    };
    let DepthRange::Receivers {
        near: inner_near,
        far: inner_far,
    } = inner.depth
    else {
        panic!("inner range has no receivers");
    };
    assert!(inner_near > near, "near bound did not contract");
    assert!(inner_far < far, "far bound did not contract");
}
