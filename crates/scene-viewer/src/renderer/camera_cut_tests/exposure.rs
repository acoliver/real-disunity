use super::*;
use std::time::Duration;
use worldrunner_render_plan::{SceneAdaptiveExposure, SceneHdrPost};
use worldrunner_render_post::{FrameConfig, Readback};

fn source() -> RenderScene {
    let base = crate::tests::scene(false);
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
    .unwrap()
    .with_hdr_post(SceneHdrPost {
        tone_map: None,
        grading: None,
        adaptive_exposure: Some(SceneAdaptiveExposure {
            metering: None,
            log_min_milli: -8_000,
            log_max_milli: 8_000,
            low_percentile_micro: 0,
            high_percentile_micro: 1_000_000,
            key_micro: 180_000,
            min_exposure_micro: 10_000,
            max_exposure_micro: 8_000_000,
            speed_up_milli: 3_000,
            speed_down_milli: 3_000,
            fixed_delta_microseconds: None,
            fixed_exposure_micro: None,
        }),
        bloom: None,
        fxaa_subpixel_micro: None,
        fxaa_quality: None,
    })
    .unwrap();
    // A uniform sky isolates exposure from the changed reconstruction samples on a cut.
    base.with_objects_and_effects(vec![], Some(effects))
        .unwrap()
}

fn frame(renderer: &mut Renderer, host: &mut RetainedHost, time: Duration, cut: bool) -> Vec<u8> {
    renderer.set_clock(crate::RenderClock::Fixed(time));
    if cut {
        let options = host.options();
        host.apply(RetainedUpdate::camera_with_continuity(
            options.pose,
            options.camera,
            options.camera_convention,
            CameraContinuity::Cut,
        ))
        .unwrap();
    }
    let (view, ready) = prepare(renderer, host);
    if cut {
        assert!(!renderer.presentation.scheduler.has_temporal_history());
        assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 0);
    }
    let texture = ready.frame.texture.clone();
    let output = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let prepared = renderer.encode_frame(&view, None, &output, None).unwrap();
    renderer
        .execution
        .submit_prepared(ready, prepared, &renderer.context)
        .unwrap();
    renderer.submitted_camera_history(host);
    renderer.streaming.captures.advance_frame();
    renderer.drain().unwrap();
    if cut {
        assert!(renderer.presentation.scheduler.has_temporal_history());
        assert_eq!(renderer.presentation.scheduler.projection_jitter_index(), 1);
        assert_eq!(
            renderer.temporal_history_reset(),
            TemporalHistoryReset::CameraCut
        );
    }
    Readback::new(
        renderer.context.device(),
        FrameConfig::new([64, 64], texture.format(), 1),
        64 * 64 * 4,
    )
    .unwrap()
    .read(
        renderer.context.device(),
        renderer.context.submission(),
        &texture,
    )
    .unwrap()
}

#[test]
fn retained_unchanged_pose_cut_preserves_warm_auto_exposure_at_zero_and_nonzero_delta() {
    use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
    for profile in [
        crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        crate::FrameProfile::HdrUpscaled {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
            scale: RenderScale::new(500_000).unwrap(),
            upscale: UpscaleSettings {
                mode: UpscaleMode::Temporal,
                sharpness: None,
            },
        },
    ] {
        for shadows in [None, Some(retained_tests::shadow_settings())] {
            preserves_exposure(profile, shadows);
        }
    }
}

fn preserves_exposure(profile: crate::FrameProfile, shadows: Option<DirectionalShadowSettings>) {
    let (mut renderer, mut host) = setup_with_scene(profile, source(), shadows);
    let (mut control, mut control_host) = setup_with_scene(profile, source(), shadows);
    let initial = frame(&mut renderer, &mut host, Duration::ZERO, false);
    assert_eq!(
        initial,
        frame(&mut control, &mut control_host, Duration::ZERO, false)
    );
    let mut warmed = initial.clone();
    for step in 1..=8 {
        let time = Duration::from_millis(step * 250);
        warmed = frame(&mut renderer, &mut host, time, false);
        assert_eq!(warmed, frame(&mut control, &mut control_host, time, false));
    }
    assert_ne!(
        initial, warmed,
        "automatic exposure must adapt away from its unity seed"
    );
    assert!(warmed.chunks_exact(4).all(|p| p == &warmed[..4]));
    for time in [Duration::from_secs(2), Duration::from_micros(2_016_667)] {
        assert!(renderer.presentation.scheduler.has_temporal_history());
        assert!(renderer.presentation.scheduler.projection_jitter_index() > 1);
        let expected = frame(&mut control, &mut control_host, time, false);
        let actual = frame(&mut renderer, &mut host, time, true);
        assert_eq!(
            actual, expected,
            "a cut must preserve the continuous exposure result"
        );
        if time == Duration::from_secs(2) {
            assert_eq!(
                actual, warmed,
                "zero delta must preserve the adapted brightness"
            );
        }
        // Warm reconstruction again before the next cut without advancing exposure time.
        frame(&mut renderer, &mut host, time, false);
        frame(&mut control, &mut control_host, time, false);
    }
}
