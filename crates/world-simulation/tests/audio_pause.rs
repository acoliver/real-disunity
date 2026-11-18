#[path = "audio_pause/ducking.rs"]
mod ducking;
#[path = "audio_pause/support.rs"]
mod support;
use support::{bus, clip, options, render};
use worldrunner_world_simulation::audio::{
    AudioCommand, AudioError, AudioMixer, AudioReply, ExistingVoices, MAX_VOICES, PlaybackOptions,
};

#[test]
fn final_clear_after_no_sweep_does_not_restart_a_playing_voice() {
    let mut mixer = AudioMixer::new(8).unwrap();
    let sound = clip(&mut mixer, &[0.125, 0.25, 0.375, 0.5], 8);
    let voice = mixer.play(&sound, 1.0).unwrap();
    mixer.pause(1, ExistingVoices::Continue);
    assert_eq!(render(&mut mixer, 1), [[0.125; 2]]);
    mixer.resume(1);
    mixer.resume(u32::MAX);
    assert_eq!(render(&mut mixer, 3), [[0.25; 2], [0.375; 2], [0.5; 2]]);
    assert_eq!(mixer.take_finished(), Some(voice));
    assert_eq!(mixer.take_finished(), None);
}

#[test]
fn suspension_retains_exact_rational_cursor_and_cannot_finish() {
    for (input, output) in [(8, 8), (3, 8), (8, 3), (44100, 48000), (48000, 44100)] {
        let mut mixer = AudioMixer::new(output).unwrap();
        let mut reference = AudioMixer::new(output).unwrap();
        let source = [0.125, -0.25, 0.75, -0.5, 0.25, 0.625, -0.75];
        let sound = clip(&mut mixer, &source, input);
        let control = clip(&mut reference, &source, input);
        let voice = mixer.play(&sound, 1.0).unwrap();
        reference.play(&control, 1.0).unwrap();
        assert_eq!(render(&mut mixer, 1), render(&mut reference, 1));
        mixer.pause(1, ExistingVoices::Suspend);
        assert_eq!(render(&mut mixer, 64), vec![[0.0; 2]; 64]);
        assert_eq!(mixer.take_finished(), None);
        assert_eq!(mixer.active_voices(), 1);
        mixer.resume(1);
        assert_eq!(render(&mut mixer, 64), render(&mut reference, 64));
        assert_eq!(mixer.take_finished(), Some(voice));
        assert_eq!(mixer.take_finished(), None);
    }
}

#[test]
fn no_sweep_blocks_only_admission_until_repeated_or_zero_sweep() {
    for sweep_bits in [0, 2] {
        let mut mixer = AudioMixer::new(8).unwrap();
        let sound = clip(&mut mixer, &[0.125, 0.25, 0.375, 0.5], 8);
        let voice = mixer.play(&sound, 1.0).unwrap();
        mixer.pause(0, ExistingVoices::Suspend);
        assert_eq!(mixer.pause_mask(), 0);
        assert_eq!(render(&mut mixer, 1), [[0.125; 2]]);
        mixer.pause(2, ExistingVoices::Continue);
        assert_eq!(mixer.play(&sound, 1.0), Err(AudioError::Paused));
        assert_eq!(render(&mut mixer, 1), [[0.25; 2]]);
        mixer.pause(sweep_bits, ExistingVoices::Suspend);
        mixer.pause(0, ExistingVoices::Continue);
        assert_eq!(render(&mut mixer, 12), vec![[0.0; 2]; 12]);
        mixer.resume(0);
        mixer.resume(1);
        assert_eq!(mixer.pause_mask(), 2);
        assert_eq!(render(&mut mixer, 1), [[0.0; 2]]);
        mixer.resume(2);
        assert_eq!(render(&mut mixer, 2), [[0.375; 2], [0.5; 2]]);
        assert_eq!(mixer.take_finished(), Some(voice));
        mixer.resume(u32::MAX);
        assert_eq!(mixer.pause_mask(), 0);
        assert_eq!(mixer.active_voices(), 0);
    }
}

#[test]
fn reasons_are_u32_sets_and_only_final_clear_releases_suspension() {
    for bits in [1, 2, 4, 0x8000_0000, 0xa5a5_5a5a, u32::MAX] {
        let mut mixer = AudioMixer::new(8).unwrap();
        let sound = clip(&mut mixer, &[0.25; 4], 8);
        mixer.play(&sound, 1.0).unwrap();
        mixer.pause(bits, ExistingVoices::Suspend);
        mixer.pause(bits, ExistingVoices::Continue);
        assert_eq!(mixer.pause_mask(), bits);
        mixer.resume(0);
        mixer.resume(!bits);
        assert_eq!(mixer.pause_mask(), bits);
        assert_eq!(render(&mut mixer, 1), [[0.0; 2]]);
        mixer.resume(bits);
        assert_eq!(mixer.pause_mask(), 0);
        assert_eq!(render(&mut mixer, 1), [[0.25; 2]]);
    }
    let mut mixer = AudioMixer::new(8).unwrap();
    let sound = clip(&mut mixer, &[0.5; 4], 8);
    mixer.play(&sound, 1.0).unwrap();
    mixer.pause(1, ExistingVoices::Suspend);
    mixer.pause(2 | 4 | 0x8000_0000, ExistingVoices::Continue);
    for bit in [1, 2, 0x8000_0000] {
        mixer.resume(bit);
        assert_ne!(mixer.pause_mask(), 0);
        assert_eq!(render(&mut mixer, 1), [[0.0; 2]]);
    }
    mixer.resume(4);
    assert_eq!(render(&mut mixer, 1), [[0.5; 2]]);
}

#[test]
fn eligibility_is_per_voice_on_the_same_bus_and_endings_keep_identity_order() {
    let mut mixer = AudioMixer::new(8).unwrap();
    bus(&mut mixer, "shared", 0.5, false, 0);
    let sound = clip(&mut mixer, &[0.125, 0.25, 0.375, 0.5], 8);
    let eligible = mixer.play_on_bus(&sound, 1.0, "shared").unwrap();
    let independent = mixer
        .play_with_options(&sound, options(Some("shared"), false))
        .unwrap();
    mixer.pause(1, ExistingVoices::Suspend);
    assert_eq!(
        render(&mut mixer, 4),
        [[0.0625; 2], [0.125; 2], [0.1875; 2], [0.25; 2]]
    );
    assert_eq!(mixer.take_finished(), Some(independent));
    assert_eq!(mixer.take_finished(), None);
    let late = mixer
        .play_with_options(&sound, options(Some("shared"), false))
        .unwrap();
    mixer.resume(1);
    assert_eq!(
        render(&mut mixer, 4),
        [[0.125; 2], [0.25; 2], [0.375; 2], [0.5; 2]]
    );
    assert_eq!(mixer.take_finished(), Some(eligible));
    assert_eq!(mixer.take_finished(), Some(late));
}

#[test]
fn admission_keeps_validation_capacity_and_never_defers_starts() {
    let mut mixer = AudioMixer::new(8).unwrap();
    let sound = clip(&mut mixer, &[0.25; 4], 8);
    let mut other = AudioMixer::new(8).unwrap();
    let foreign = clip(&mut other, &[0.25; 4], 8);
    mixer.pause(1, ExistingVoices::Continue);
    assert_eq!(mixer.play(&sound, f32::NAN), Err(AudioError::InvalidGain));
    assert_eq!(mixer.play(&foreign, 1.0), Err(AudioError::UnknownClip));
    assert_eq!(
        mixer.play_on_bus(&sound, 1.0, "missing"),
        Err(AudioError::UnknownBus("missing".into()))
    );
    assert_eq!(mixer.play(&sound, 1.0), Err(AudioError::Paused));
    assert_eq!(
        mixer.play_with_options(&sound, PlaybackOptions::new(1.0)),
        Err(AudioError::Paused)
    );
    assert_eq!(mixer.occupied_voices(), 0);
    for _ in 0..MAX_VOICES {
        mixer
            .play_with_options(&sound, options(None, false))
            .unwrap();
    }
    assert_eq!(
        mixer.play_with_options(&sound, options(None, false)),
        Err(AudioError::VoiceLimit)
    );
    assert_eq!(mixer.play(&sound, 1.0), Err(AudioError::VoiceLimit));
    mixer.resume(1);
    render(&mut mixer, 4);
    assert_eq!(mixer.active_voices(), 0);
    assert_eq!(
        std::iter::from_fn(|| mixer.take_finished()).count(),
        MAX_VOICES
    );
    assert_eq!(render(&mut mixer, 4), [[0.0; 2]; 4]);
}

#[test]
fn suspended_capacity_stop_gain_mute_and_mask_lifetime_are_independent() {
    let mut mixer = AudioMixer::new(8).unwrap();
    assert_eq!(mixer.pause_mask(), 0);
    bus(&mut mixer, "shared", 1.0, false, 0);
    let sound = clip(&mut mixer, &[0.25, 0.5, 0.75], 8);
    let voices: Vec<_> = (0..MAX_VOICES)
        .map(|_| mixer.play_on_bus(&sound, 1.0, "shared").unwrap())
        .collect();
    mixer.pause(4, ExistingVoices::Suspend);
    assert_eq!(mixer.active_voices(), MAX_VOICES);
    assert_eq!(mixer.set_output_rate(16), Err(AudioError::OutputBusy));
    assert_eq!(
        mixer.play_with_options(&sound, options(None, false)),
        Err(AudioError::VoiceLimit)
    );
    for voice in &voices[1..] {
        assert!(mixer.stop(voice));
    }
    mixer.set_master_gain(0.5).unwrap();
    mixer.set_mute(true);
    bus(&mut mixer, "shared", 0.5, true, 0);
    assert_eq!(render(&mut mixer, 12), vec![[0.0; 2]; 12]);
    mixer.set_mute(false);
    bus(&mut mixer, "shared", 0.5, false, 0);
    assert_eq!(render(&mut mixer, 1), [[0.0; 2]]);
    mixer.resume(4);
    assert_eq!(render(&mut mixer, 1), [[0.0625; 2]]);
    bus(&mut mixer, "shared", 0.5, true, 0);
    assert_eq!(render(&mut mixer, 1), [[0.0; 2]]);
    mixer.set_mute(true);
    render(&mut mixer, 1);
    mixer.pause(2, ExistingVoices::Suspend);
    assert_eq!(mixer.take_finished(), Some(voices[0].clone()));
    assert_eq!(mixer.take_finished(), None);
    mixer.clear_voices();
    mixer.set_output_rate(16).unwrap();
    assert_eq!(mixer.pause_mask(), 2);
    assert_eq!(mixer.play(&sound, 1.0), Err(AudioError::Paused));
    mixer.resume(2);
    let stopped = mixer.play(&sound, 1.0).unwrap();
    mixer.pause(1, ExistingVoices::Suspend);
    assert!(mixer.stop(&stopped));
    assert_eq!(mixer.take_finished(), None);
    mixer.clear_voices();
    assert_eq!(mixer.pause_mask(), 1);
}

#[test]
fn commands_forward_pause_query_options_and_resume() {
    let mut mixer = AudioMixer::new(8).unwrap();
    let sound = clip(&mut mixer, &[0.25; 2], 8);
    assert_eq!(
        mixer.command(AudioCommand::Pause {
            bits: 1,
            existing: ExistingVoices::Suspend
        }),
        Ok(AudioReply::Applied)
    );
    assert_eq!(
        mixer.command(AudioCommand::PauseMask),
        Ok(AudioReply::PauseMask(1))
    );
    assert_eq!(
        mixer.command(AudioCommand::Play {
            clip: sound.clone(),
            gain: 1.0
        }),
        Err(AudioError::Paused)
    );
    assert!(matches!(
        mixer.command(AudioCommand::PlayWithOptions {
            clip: sound.clone(),
            options: options(None, false)
        }),
        Ok(AudioReply::Playing(_))
    ));
    assert_eq!(
        mixer.command(AudioCommand::Resume { bits: 1 }),
        Ok(AudioReply::Applied)
    );
    assert!(matches!(
        mixer.command(AudioCommand::PlayWithOptions {
            clip: sound,
            options: PlaybackOptions::new(1.0)
        }),
        Ok(AudioReply::Playing(_))
    ));
    assert_eq!(render(&mut mixer, 2), [[0.5; 2]; 2]);
}

fn partitioned(partition: usize) -> (Vec<[f32; 2]>, Vec<usize>) {
    let mut mixer = AudioMixer::new(8).unwrap();
    let sound = clip(&mut mixer, &[0.125, -0.25, 0.375, -0.5, 0.625], 3);
    let eligible = mixer.play(&sound, 1.0).unwrap();
    let independent = mixer
        .play_with_options(&sound, options(None, false))
        .unwrap();
    let identities = [eligible, independent];
    let mut out = Vec::new();
    for (frames, control) in [(3, 0), (7, 1), (5, 2), (4, 3), (20, 4)] {
        match control {
            1 => mixer.pause(2, ExistingVoices::Continue),
            2 => mixer.pause(0, ExistingVoices::Suspend),
            3 => mixer.resume(1),
            4 => mixer.resume(2),
            _ => (),
        }
        let mut left = frames;
        while left > 0 {
            let count = left.min(partition);
            out.extend(render(&mut mixer, count));
            left -= count;
        }
    }
    let endings = std::iter::from_fn(|| mixer.take_finished())
        .map(|voice| {
            identities
                .iter()
                .position(|handle| *handle == voice)
                .unwrap()
        })
        .collect();
    (out, endings)
}

#[test]
fn fixed_control_boundaries_are_render_block_partition_independent() {
    let expected = partitioned(64);
    assert_eq!(expected.1, [1, 0]);
    for partition in [1, 2, 3, 5, 8] {
        assert_eq!(partitioned(partition), expected);
    }
}
