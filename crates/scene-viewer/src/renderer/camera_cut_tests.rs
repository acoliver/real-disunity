use super::*;
use crate::{CameraContinuity, RetainedHost, RetainedUpdate, TemporalHistoryReset};
use worldrunner_render_plan::{SceneAntiAlias, SceneEffectsParts};

mod exposure;

fn setup() -> (Renderer, RetainedHost) {
    setup_with_profile(crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    })
}

fn setup_with_profile(profile: crate::FrameProfile) -> (Renderer, RetainedHost) {
    let base = retained_tests::source();
    let effects = base.effects().unwrap();
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *effects.sky(),
        sun: *effects.sun(),
        ambient: *effects.ambient(),
        shadows: effects.shadows().clone(),
        camera: *effects.camera(),
        grading: *effects.grading(),
        post: vec![],
        anti_alias: SceneAntiAlias::taa(900_000),
        ssao: None,
    })
    .unwrap();
    let scene = base
        .with_objects_and_effects(base.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    setup_with_scene(profile, scene, None)
}

fn setup_with_scene(
    profile: crate::FrameProfile,
    scene: RenderScene,
    shadows: Option<DirectionalShadowSettings>,
) -> (Renderer, RetainedHost) {
    let options = crate::ViewerOptions {
        size: [64, 64],
        frame_profile: profile,
        shadows,
        render_clock: crate::RenderClock::Fixed(std::time::Duration::ZERO),
        ..crate::tests::options()
    };
    let mut host = RetainedHost::new(scene, options).unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host)).unwrap();
    renderer.set_clock(host.options().render_clock);
    (renderer, host)
}

fn camera(host: &RetainedHost, continuity: CameraContinuity) -> RetainedUpdate {
    let options = host.options();
    let mut pose = options.pose;
    pose.eye_millimeters[0] += 500;
    RetainedUpdate::camera_with_continuity(
        pose,
        options.camera,
        options.camera_convention,
        continuity,
    )
}

fn submit(renderer: &mut Renderer, host: &mut RetainedHost) {
    renderer.drain().unwrap();
    assert!(renderer.render_retained(host).unwrap());
    renderer.drain().unwrap();
}

fn prepare(renderer: &mut Renderer, host: &mut RetainedHost) -> (SceneView, execution::ReadyFrame) {
    renderer.drain().unwrap();
    let ready = renderer
        .execution
        .acquire(&renderer.context, || Ok(()))
        .unwrap()
        .unwrap();
    renderer.publish_retained(host).unwrap();
    let view = host.view().unwrap().unwrap();
    renderer
        .prepare_frame(&view, ready.admission.slot())
        .unwrap();
    renderer.prepare_camera_history(host);
    (view, ready)
}

#[test]
fn retained_cut_resets_warm_history_and_jitter_before_first_submission() {
    let (mut renderer, mut host) = setup();
    submit(&mut renderer, &mut host);
    submit(&mut renderer, &mut host);
    assert!(renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 2);
    host.apply(camera(&host, CameraContinuity::Cut)).unwrap();
    host.apply(camera(&host, CameraContinuity::Cut)).unwrap();
    let (view, ready) = prepare(&mut renderer, &mut host);
    assert!(!renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 0);
    let output = ready
        .frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let prepared = renderer.encode_frame(&view, None, &output, None).unwrap();
    renderer
        .execution
        .submit_prepared(ready, prepared, &renderer.context)
        .unwrap();
    renderer.submitted_camera_history(&mut host);
    renderer.streaming.captures.advance_frame();
    assert!(renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 1);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::CameraCut
    );
    submit(&mut renderer, &mut host);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::None
    );
}

#[test]
fn retained_continuous_camera_keeps_history_and_matches_existing_path_bytes() {
    let (mut control, mut old) = setup();
    let (mut renderer, mut host) = setup();
    for _ in 0..2 {
        submit(&mut control, &mut old);
        submit(&mut renderer, &mut host);
    }
    host.apply(camera(&host, CameraContinuity::Continuous))
        .unwrap();
    let options = host.options();
    old.apply(RetainedUpdate::Camera {
        pose: options.pose,
        lens: options.camera,
        convention: options.camera_convention,
    })
    .unwrap();
    let (view, ready) = prepare(&mut renderer, &mut host);
    assert!(renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 2);
    drop((view, ready));
    let first = host.request_capture(host.accepted_generation()).unwrap();
    let second = old.request_capture(old.accepted_generation()).unwrap();
    submit(&mut renderer, &mut host);
    submit(&mut control, &mut old);
    renderer.collect_retained(&mut host).unwrap();
    control.collect_retained(&mut old).unwrap();
    let capture = |host: &mut RetainedHost, id| {
        let crate::RetainedEvent::Capture {
            id: actual,
            outcome: crate::RetainedCaptureOutcome::Completed { image, .. },
            ..
        } = host.captures.take_event().unwrap()
        else {
            panic!("capture")
        };
        assert_eq!(actual, id);
        image
    };
    assert_eq!(capture(&mut host, first), capture(&mut old, second));
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::None
    );
}

#[test]
fn retained_cut_survives_abandoned_and_cancelled_frames() {
    let (mut renderer, mut host) = setup();
    submit(&mut renderer, &mut host);
    host.apply(camera(&host, CameraContinuity::Cut)).unwrap();
    let (view, ready) = prepare(&mut renderer, &mut host);
    drop((view, ready));
    let (view, ready) = prepare(&mut renderer, &mut host);
    let output = ready
        .frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .encode_frame(&view, None, &output, None)
        .unwrap()
        .cancel();
    drop(ready);
    assert!(!renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 0);
    assert_eq!(
        host.readiness.camera_history.reset_reason(),
        TemporalHistoryReset::CameraCut
    );
    submit(&mut renderer, &mut host);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::CameraCut
    );
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 1);
}

#[test]
fn retained_temporal_upscale_cut_restarts_submission_history() {
    use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
    let (mut renderer, mut host) = setup_with_profile(crate::FrameProfile::HdrUpscaled {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
        scale: RenderScale::new(500_000).unwrap(),
        upscale: UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        },
    });
    submit(&mut renderer, &mut host);
    submit(&mut renderer, &mut host);
    assert!(renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 2);
    host.apply(camera(&host, CameraContinuity::Cut)).unwrap();
    let (view, ready) = prepare(&mut renderer, &mut host);
    assert!(!renderer.presentation.scheduler.has_temporal_history());
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 0);
    drop((view, ready));
    submit(&mut renderer, &mut host);
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 1);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::CameraCut
    );
    submit(&mut renderer, &mut host);
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 2);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::None
    );
}

#[test]
fn retained_cut_capture_freezes_reset_after_later_frames() {
    let (mut renderer, mut host) = setup();
    submit(&mut renderer, &mut host);
    submit(&mut renderer, &mut host);
    host.apply(camera(&host, CameraContinuity::Cut)).unwrap();
    let generation = host.accepted_generation();
    let id = host.request_capture(generation).unwrap();
    submit(&mut renderer, &mut host);
    assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 1);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::CameraCut
    );
    submit(&mut renderer, &mut host);
    assert_eq!(
        renderer.temporal_history_reset(),
        TemporalHistoryReset::None
    );
    renderer.collect_retained(&mut host).unwrap();
    let crate::RetainedEvent::Capture {
        id: actual,
        outcome: crate::RetainedCaptureOutcome::Completed { observation, .. },
        ..
    } = host.captures.take_event().unwrap()
    else {
        panic!("completed capture")
    };
    assert_eq!(actual, id);
    assert_eq!(observation.generation, generation);
    assert_eq!(
        observation.temporal_history_reset,
        TemporalHistoryReset::CameraCut
    );
}
