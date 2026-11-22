use super::{test_support::clip, *};
use std::sync::atomic::Ordering;

#[test]
fn unopened_and_failed_devices_reject_playback_and_report_status() {
    let mut audio = AudioService::default();
    let clip = audio.load_wav(&clip()).unwrap();
    for reason in [
        AudioUnavailable::NotStarted,
        AudioUnavailable::NoDevice,
        AudioUnavailable::OpenFailed,
        AudioUnavailable::UnsupportedConfig,
    ] {
        let status = AudioStatus::Unavailable(reason);
        let failure = audio.start_device(|_| Err((reason, "injected device failure".into())));
        assert!(matches!(failure, Err(AudioServiceError::Unavailable(found)) if found == status));
        assert_eq!(audio.device_error(), Some("injected device failure"));
        assert_eq!(audio.status(), status);
        assert!(
            matches!(audio.play(&clip, 1.0), Err(AudioServiceError::Unavailable(found)) if found == status)
        );
        assert!(audio.next_wakeup().is_some());
        assert_eq!(
            audio.take_event(),
            Some(RetainedEvent::AudioStatusChanged { status })
        );
        assert_eq!(audio.take_event(), None);
    }
}
#[test]
fn device_loss_is_visible_before_dispatch_and_cancels_unplayed_completions() {
    let mut audio = AudioService::default();
    let clip = audio.load_wav(&clip()).unwrap();
    audio.status = AudioStatus::Ready { sample_rate: 48000 };
    let ring = ring::Ring::new();
    audio.ring = Some(ring.clone());
    audio.play(&clip, 1.0).unwrap();
    ring.lost.store(true, Ordering::Release);
    let status = AudioStatus::Unavailable(AudioUnavailable::DeviceLost);
    assert_eq!(audio.status(), status);
    assert!(audio.play(&clip, 1.0).is_err());
    assert_eq!(
        audio.take_event(),
        Some(RetainedEvent::AudioStatusChanged { status })
    );
    assert_eq!(audio.take_event(), None);
    assert_eq!(audio.state.lock().unwrap().mixer.occupied_voices(), 0);
}
#[test]
fn completion_waits_for_callback_consumption_and_retains_voice_capacity() {
    let mut audio = AudioService::default();
    let clip = audio.load_wav(&clip()).unwrap();
    audio.status = AudioStatus::Ready { sample_rate: 48000 };
    let ring = ring::Ring::new();
    audio.ring = Some(ring.clone());
    let mut state = audio.state.lock().unwrap();
    for _ in 0..MAX_VOICES {
        let voice = state.mixer.play(&clip, 1.0).unwrap();
        assert!(state.mixer.stop(&voice));
        state.pending.push_back((voice, 1));
    }
    drop(state);
    assert!(matches!(
        audio.play(&clip, 1.0),
        Err(AudioServiceError::Audio(AudioError::VoiceLimit))
    ));
    assert_eq!(audio.take_event(), None);
    assert!(matches!(
        audio.start_device(|_| panic!("device startup must not be attempted")),
        Err(AudioServiceError::Busy)
    ));
    ring.push(&[[0.0; 2]]);
    ring.fill(&mut [0_f32; 2], 2);
    assert!(matches!(
        audio.take_event(),
        Some(RetainedEvent::AudioFinished { .. })
    ));
    assert!(audio.play(&clip, 1.0).is_ok());
}
#[test]
fn null_sink_carries_fractional_frames_and_cannot_open_a_device() {
    let mut audio = AudioService::default();
    audio.null_device();
    assert!(
        audio
            .start_device(|_| panic!("device startup must not be attempted"))
            .is_err()
    );
    let clip = audio.load_wav(&clip()).unwrap();
    let voice = audio.play(&clip, 1.0).unwrap();
    audio.take_event();
    audio.advance(Duration::from_nanos(999_999_999));
    assert_eq!(audio.take_event(), None);
    audio.advance(Duration::from_nanos(1));
    assert_eq!(
        audio.take_event(),
        Some(RetainedEvent::AudioFinished { voice })
    );
    audio.shutdown();
    assert_eq!(audio.status(), AudioStatus::Closed);
    assert!(audio.set_mute(true).is_err());
    assert!(audio.set_master_gain(1.0).is_err());
}

#[test]
fn null_device_commands_route_samples_configure_buses_and_keep_old_play_valid() {
    let mut audio = AudioService::default();
    audio.null_device();
    let mut bytes = clip();
    for sample in bytes[44..].chunks_exact_mut(2) {
        sample.copy_from_slice(&16384_i16.to_le_bytes());
    }
    let clip = audio.load_wav(&bytes).unwrap();
    let config = BusConfig {
        gain: 0.5,
        muted: false,
        tier: 0,
    };
    assert_eq!(
        audio
            .command(AudioCommand::ConfigureBus {
                name: "music".into(),
                config
            })
            .unwrap(),
        AudioReply::Applied
    );
    audio
        .configure_bus("speech", BusConfig { tier: 1, ..config })
        .unwrap();
    let rule = DuckingRule {
        source_buses: vec!["speech".into()],
        target_buses: vec!["music".into()],
        gain: 0.5,
        attack: Duration::ZERO,
        hold: Duration::ZERO,
        release: Duration::ZERO,
        envelope: EnvelopeMode::LinearGain,
        detector: DuckingDetector::Activity,
    };
    audio
        .command(AudioCommand::DuckingRules(vec![rule]))
        .unwrap();
    assert!(matches!(
        audio
            .command(AudioCommand::PlayOnBus {
                clip: clip.clone(),
                gain: 1.0,
                bus: "music".into()
            })
            .unwrap(),
        AudioReply::Playing(_)
    ));
    let source = audio.play_on_bus(&clip, 0.0, "speech").unwrap();
    let old = audio.play(&clip, 0.25).unwrap();
    let mut out = [[0.0; 2]];
    audio.state.lock().unwrap().mixer.render(&mut out);
    assert_eq!(out, [[0.25; 2]]);
    assert!(audio.stop(&old));
    assert!(audio.stop(&source));
    audio.replace_ducking_rules(vec![]).unwrap();
    audio.state.lock().unwrap().mixer.render(&mut out);
    assert_eq!(out, [[0.25; 2]]);
    assert!(
        matches!(audio.play_on_bus(&clip, 1.0, "missing"), Err(AudioServiceError::Audio(AudioError::UnknownBus(name))) if name == "missing")
    );
    audio.shutdown();
    assert!(audio.configure_bus("music", config).is_err());
    assert!(audio.replace_ducking_rules(vec![]).is_err());
}

#[test]
fn null_device_advances_ducking_release_during_silence() {
    let mut audio = AudioService::default();
    audio.null_device();
    for (name, tier) in [("music", 0), ("speech", 1)] {
        audio
            .configure_bus(
                name,
                BusConfig {
                    gain: 1.0,
                    muted: false,
                    tier,
                },
            )
            .unwrap();
    }
    let mut bytes = clip();
    for sample in bytes[44..].chunks_exact_mut(2) {
        sample.copy_from_slice(&16384_i16.to_le_bytes());
    }
    let clip = audio.load_wav(&bytes).unwrap();
    audio
        .replace_ducking_rules(vec![DuckingRule {
            source_buses: vec!["speech".into()],
            target_buses: vec!["music".into()],
            gain: 0.25,
            attack: Duration::ZERO,
            hold: Duration::ZERO,
            release: Duration::from_millis(250),
            envelope: EnvelopeMode::LinearGain,
            detector: DuckingDetector::Activity,
        }])
        .unwrap();
    let source = audio.play_on_bus(&clip, 0.0, "speech").unwrap();
    audio.advance(Duration::from_millis(1));
    audio.stop(&source);
    audio.advance(Duration::from_millis(250));
    audio.play_on_bus(&clip, 1.0, "music").unwrap();
    let mut out = [[0.0; 2]];
    audio.state.lock().unwrap().mixer.render(&mut out);
    assert_eq!(out, [[0.5; 2]]);
}
