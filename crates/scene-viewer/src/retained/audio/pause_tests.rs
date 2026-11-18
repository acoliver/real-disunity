use super::{test_support::clip, *};
use crate::{ExistingVoices, PlaybackOptions};

fn independent() -> PlaybackOptions {
    PlaybackOptions {
        pause_eligible: false,
        ..PlaybackOptions::new(1.0)
    }
}

#[test]
fn pause_before_null_startup_survives_setup_and_closed_controls_fail() {
    let mut audio = AudioService::default();
    audio.pause(0x8000_0001, ExistingVoices::Suspend).unwrap();
    assert_eq!(audio.pause_mask().unwrap(), 0x8000_0001);
    audio.null_device();
    assert_eq!(audio.pause_mask().unwrap(), 0x8000_0001);
    let sound = audio.load_wav(&clip()).unwrap();
    assert!(matches!(
        audio.play(&sound, 1.0),
        Err(AudioServiceError::Audio(AudioError::Paused))
    ));
    let voice = audio.play_with_options(&sound, independent()).unwrap();
    audio.take_event();
    audio.advance(Duration::from_secs(1));
    assert_eq!(
        audio.take_event(),
        Some(RetainedEvent::AudioFinished { voice })
    );
    audio.resume(1).unwrap();
    assert_eq!(
        audio.command(AudioCommand::PauseMask).unwrap(),
        AudioReply::PauseMask(0x8000_0000)
    );
    audio.resume(0x8000_0000).unwrap();
    audio.shutdown();
    assert!(matches!(
        audio.pause(1, ExistingVoices::Suspend),
        Err(AudioServiceError::Unavailable(AudioStatus::Closed))
    ));
    assert!(matches!(
        audio.resume(1),
        Err(AudioServiceError::Unavailable(AudioStatus::Closed))
    ));
    assert!(matches!(
        audio.pause_mask(),
        Err(AudioServiceError::Unavailable(AudioStatus::Closed))
    ));
    for control in [
        AudioCommand::PauseMask,
        AudioCommand::Pause {
            bits: 1,
            existing: ExistingVoices::Suspend,
        },
        AudioCommand::Resume { bits: 1 },
    ] {
        assert!(matches!(
            audio.command(control),
            Err(AudioServiceError::Unavailable(AudioStatus::Closed))
        ));
    }
    assert!(matches!(
        audio.play_with_options(&sound, independent()),
        Err(AudioServiceError::Unavailable(AudioStatus::Closed))
    ));
}

#[test]
fn null_commands_suspend_without_completion_and_resume_the_same_cursor() {
    let mut audio = AudioService::default();
    audio.null_device();
    let mut bytes = clip();
    for (sample, value) in bytes[44..]
        .chunks_exact_mut(2)
        .zip([4096_i16, 8192, 12288, 16384])
    {
        sample.copy_from_slice(&value.to_le_bytes());
    }
    let sound = audio.load_wav(&bytes).unwrap();
    audio
        .configure_bus(
            "shared",
            BusConfig {
                gain: 1.0,
                muted: false,
                tier: 0,
            },
        )
        .unwrap();
    let eligible = audio.play_on_bus(&sound, 1.0, "shared").unwrap();
    audio.take_event();
    audio.advance(Duration::from_millis(250));
    assert_eq!(
        audio
            .command(AudioCommand::Pause {
                bits: 2,
                existing: ExistingVoices::Continue
            })
            .unwrap(),
        AudioReply::Applied
    );
    assert!(matches!(
        audio.command(AudioCommand::PlayWithOptions {
            clip: sound.clone(),
            options: PlaybackOptions::new(1.0)
        }),
        Err(AudioServiceError::Audio(AudioError::Paused))
    ));
    audio.advance(Duration::from_millis(250));
    audio.pause(0, ExistingVoices::Suspend).unwrap();
    let AudioReply::Playing(independent) = audio
        .command(AudioCommand::PlayWithOptions {
            clip: sound,
            options: PlaybackOptions {
                bus: Some("shared".into()),
                ..independent()
            },
        })
        .unwrap()
    else {
        panic!("voice reply")
    };
    audio.advance(Duration::from_secs(2));
    assert_eq!(
        audio.take_event(),
        Some(RetainedEvent::AudioFinished { voice: independent })
    );
    assert_eq!(audio.take_event(), None);
    audio.resume(1).unwrap();
    audio.advance(Duration::from_secs(1));
    assert_eq!(audio.take_event(), None);
    audio.command(AudioCommand::Resume { bits: 2 }).unwrap();
    let mut out = [[0.0; 2]];
    audio.state.lock().unwrap().mixer.render(&mut out);
    assert_eq!(out, [[0.375; 2]]);
    audio.advance(Duration::from_millis(500));
    assert_eq!(
        audio.take_event(),
        Some(RetainedEvent::AudioFinished { voice: eligible })
    );
    assert_eq!(audio.take_event(), None);
}

#[test]
fn options_playback_preserves_unstarted_status_and_pending_completion_capacity() {
    let mut audio = AudioService::default();
    let sound = audio.load_wav(&clip()).unwrap();
    assert!(matches!(
        audio.command(AudioCommand::PlayWithOptions {
            clip: sound.clone(),
            options: independent()
        }),
        Err(AudioServiceError::Unavailable(AudioStatus::Unavailable(
            AudioUnavailable::NotStarted
        )))
    ));
    audio.null_device();
    audio.take_event();
    {
        let mut state = audio.state.lock().unwrap();
        for _ in 0..MAX_VOICES {
            let voice = state.mixer.play(&sound, 1.0).unwrap();
            assert!(state.mixer.stop(&voice));
            state.pending.push_back((voice, 1));
        }
    }
    audio.pause(1, ExistingVoices::Suspend).unwrap();
    assert!(matches!(
        audio.play_with_options(&sound, independent()),
        Err(AudioServiceError::Audio(AudioError::VoiceLimit))
    ));
    assert!(matches!(
        audio.command(AudioCommand::PlayWithOptions {
            clip: sound.clone(),
            options: independent()
        }),
        Err(AudioServiceError::Audio(AudioError::VoiceLimit))
    ));
    let pending = audio
        .state
        .lock()
        .unwrap()
        .pending
        .front()
        .unwrap()
        .0
        .clone();
    assert!(audio.stop(&pending));
    assert!(audio.play_with_options(&sound, independent()).is_ok());
    audio.null_device();
    assert_eq!(audio.pause_mask().unwrap(), 1);
    assert_eq!(audio.state.lock().unwrap().mixer.occupied_voices(), 0);
}
