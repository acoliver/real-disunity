use worldrunner_world_simulation::audio::{AudioError, AudioMixer, Clip, MAX_VOICES};

fn wav(tag: u16, bits: u16, channels: u16, rate: u32, data: &[u8]) -> Vec<u8> {
    let align = channels * (bits / 8);
    let mut bytes = b"RIFF".to_vec();
    let size = u32::try_from(data.len()).unwrap();
    bytes.extend((36 + size + size % 2).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(tag.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * u32::from(align)).to_le_bytes());
    bytes.extend(align.to_le_bytes());
    bytes.extend(bits.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
    bytes.extend(data);
    if !data.len().is_multiple_of(2) {
        bytes.push(0);
    }
    bytes
}

#[test]
fn parses_all_encodings_and_stereo() {
    let cases = [
        (1, 8, vec![0, 128, 192]),
        (1, 16, vec![0, 128, 0, 0, 0, 64]),
        (1, 24, vec![0, 0, 128, 0, 0, 0, 0, 0, 64]),
        (
            3,
            32,
            [-1_f32, 0.0, 0.5]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
        ),
    ];
    for (tag, bits, data) in cases {
        let clip = Clip::from_wav(&wav(tag, bits, 1, 44100, &data)).unwrap();
        assert_eq!(clip.samples(), &[[-1.0; 2], [0.0; 2], [0.5; 2]]);
        assert_eq!(clip.sample_rate(), 44100);
    }
    let clip = Clip::from_wav(&wav(1, 16, 2, 48000, &[0, 64, 0, 192])).unwrap();
    assert_eq!(clip.samples(), &[[0.5, -0.5]]);
}

#[test]
fn rejects_malformed_unsupported_and_nonfinite_wav() {
    assert!(Clip::from_wav(b"RIFF").is_err());
    for (tag, bits, channels) in [(2, 16, 1), (1, 32, 1), (1, 16, 3)] {
        assert!(matches!(
            Clip::from_wav(&wav(tag, bits, channels, 48000, &[0; 12])),
            Err(AudioError::UnsupportedFormat)
        ));
    }
    assert!(Clip::from_wav(&wav(1, 16, 1, 0, &[0; 2])).is_err());
    assert!(Clip::from_wav(&wav(1, 16, 1, 48000, &[0])).is_err());
    assert!(Clip::from_wav(&wav(3, 32, 1, 48000, &f32::NAN.to_le_bytes())).is_err());
    let mut bytes = wav(1, 16, 1, 48000, &[0; 2]);
    bytes[28] ^= 1;
    assert!(Clip::from_wav(&bytes).is_err());
    let bytes = vec![0; worldrunner_world_simulation::audio::MAX_WAV_BYTES + 1];
    assert!(matches!(
        Clip::from_wav(&bytes),
        Err(AudioError::ClipTooLarge)
    ));
}

fn mixer() -> (AudioMixer, worldrunner_world_simulation::audio::ClipHandle) {
    let mut mixer = AudioMixer::new(8).unwrap();
    let bytes: Vec<_> = [0.5_f32; 4]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let clip = mixer.load_wav(&wav(3, 32, 1, 8, &bytes)).unwrap();
    (mixer, clip)
}

#[test]
fn gain_overlap_clipping_stop_and_completion() {
    let (mut mixer, clip) = mixer();
    let first = mixer.play(&clip, 1.5).unwrap();
    let second = mixer.play(&clip, 1.0).unwrap();
    let mut out = [[0.0; 2]; 2];
    mixer.render(&mut out);
    assert_eq!(out, [[1.0; 2]; 2]);
    assert!(mixer.stop(&second));
    mixer.set_master_gain(0.5).unwrap();
    mixer.render(&mut out);
    assert_eq!(out, [[0.375; 2]; 2]);
    assert_eq!(mixer.take_finished(), Some(first));
    assert_eq!(mixer.take_finished(), None);
    mixer.render(&mut out);
    assert_eq!(out, [[0.0; 2]; 2]);
}

#[test]
fn muted_voices_advance_and_limits_include_undelivered_completions() {
    let (mut mixer, clip) = mixer();
    for _ in 0..MAX_VOICES {
        mixer.play(&clip, 1.0).unwrap();
    }
    assert_eq!(mixer.play(&clip, 1.0), Err(AudioError::VoiceLimit));
    mixer.set_mute(true);
    let mut out = [[1.0; 2]; 4];
    mixer.render(&mut out);
    assert_eq!(out, [[0.0; 2]; 4]);
    assert_eq!(mixer.play(&clip, 1.0), Err(AudioError::VoiceLimit));
    assert!(mixer.take_finished().is_some());
    assert!(mixer.play(&clip, 1.0).is_ok());
}

#[test]
fn linear_resampling_known_tone_and_exact_duration() {
    let mut mixer = AudioMixer::new(8).unwrap();
    let data: Vec<_> = [0_f32, 1.0, 0.0, -1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let clip = mixer.load_wav(&wav(3, 32, 1, 4, &data)).unwrap();
    let voice = mixer.play(&clip, 1.0).unwrap();
    let mut out = [[0.0; 2]; 7];
    mixer.render(&mut out);
    assert_eq!(
        out.map(|frame| frame[0].to_bits()),
        [0.0_f32, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0].map(f32::to_bits)
    );
    assert_eq!(mixer.take_finished(), None);
    mixer.render(&mut [[0.0; 2]]);
    assert_eq!(mixer.take_finished(), Some(voice));
}

#[test]
fn foreign_handles_invalid_gain_and_output_rate_fail() {
    let (mut first, clip) = mixer();
    let (mut second, _) = mixer();
    assert_eq!(second.play(&clip, 1.0), Err(AudioError::UnknownClip));
    assert_eq!(first.play(&clip, f32::NAN), Err(AudioError::InvalidGain));
    assert_eq!(first.set_master_gain(-1.0), Err(AudioError::InvalidGain));
    assert!(AudioMixer::new(0).is_err());
    let voice = first.play(&clip, 1.0).unwrap();
    assert!(!second.stop(&voice));
}

#[test]
fn wav_chunks_require_unique_complete_payloads_and_skip_unknown_chunks() {
    let original = wav(1, 8, 1, 22050, &[192]);
    for end in 0..original.len() {
        assert!(Clip::from_wav(&original[..end]).is_err());
    }
    let mut bytes = original.clone();
    bytes.extend(b"JUNK\x01\0\0\0x\0");
    let size = u32::try_from(bytes.len() - 8).unwrap();
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    assert_eq!(Clip::from_wav(&bytes).unwrap().samples(), &[[0.5; 2]]);
    for chunk in [&original[12..36], &original[36..]] {
        let mut duplicate = original.clone();
        duplicate.extend(chunk);
        let size = u32::try_from(duplicate.len() - 8).unwrap();
        duplicate[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(matches!(
            Clip::from_wav(&duplicate),
            Err(AudioError::MalformedWav)
        ));
    }
    let mut oversized = original;
    oversized[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Clip::from_wav(&oversized),
        Err(AudioError::MalformedWav)
    ));
}

#[test]
fn clip_storage_is_bounded_and_commands_preserve_admission_rules() {
    use worldrunner_world_simulation::audio::{AudioCommand, AudioReply, MAX_CLIPS};
    let (mut mixer, clip) = mixer();
    let bytes = wav(1, 16, 1, 48000, &[0; 2]);
    for _ in 1..MAX_CLIPS {
        mixer.load_wav(&bytes).unwrap();
    }
    assert_eq!(mixer.load_wav(&bytes), Err(AudioError::ClipLimit));
    let AudioReply::Playing(voice) = mixer
        .command(AudioCommand::Play { clip, gain: 1.0 })
        .unwrap()
    else {
        panic!("play returns a voice");
    };
    assert_eq!(mixer.set_output_rate(44100), Err(AudioError::OutputBusy));
    assert_eq!(
        mixer.command(AudioCommand::Mute(true)),
        Ok(AudioReply::Applied)
    );
    assert_eq!(
        mixer.command(AudioCommand::MasterGain(0.5)),
        Ok(AudioReply::Applied)
    );
    assert_eq!(
        mixer.command(AudioCommand::Stop(voice.clone())),
        Ok(AudioReply::Stopped(true))
    );
    assert_eq!(
        mixer.command(AudioCommand::Stop(voice)),
        Ok(AudioReply::Stopped(false))
    );
    mixer.set_output_rate(44100).unwrap();
}

#[test]
fn downsampling_preserves_stereo_and_rounds_duration_up() {
    let mut mixer = AudioMixer::new(2).unwrap();
    let samples = [0.5_f32, -0.5, 0.25, -0.25, 1.0, -1.0];
    let bytes: Vec<_> = samples.into_iter().flat_map(f32::to_le_bytes).collect();
    let clip = mixer.load_wav(&wav(3, 32, 2, 3, &bytes)).unwrap();
    let voice = mixer.play(&clip, 1.0).unwrap();
    let mut out = [[0.0; 2]; 2];
    mixer.render(&mut out);
    assert_eq!(out, [[0.5, -0.5], [0.625, -0.625]]);
    assert_eq!(mixer.take_finished(), Some(voice));
}
