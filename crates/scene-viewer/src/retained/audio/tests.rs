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
        let failure = audio.start_device_with(|_| Err((reason, "injected device failure".into())));
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
        audio.start_device_with(|_| panic!("device startup must not be attempted")),
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
            .start_device_with(|_| panic!("device startup must not be attempted"))
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
