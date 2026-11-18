use super::*;

#[test]
fn paused_admission_does_not_allocate_voice_identity_or_slot() {
    let mut mixer = AudioMixer::new(8).unwrap();
    let mut wav = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
    wav.extend(8_u32.to_le_bytes());
    wav.extend(16_u32.to_le_bytes());
    wav.extend([2, 0, 16, 0]);
    wav.extend(b"data\x02\0\0\0\0\x40");
    let handle = mixer.load_wav(&wav).unwrap();
    mixer.pause(1, ExistingVoices::Suspend);
    assert_eq!(mixer.play(&handle, 1.0), Err(AudioError::Paused));
    assert_eq!(
        mixer.play_with_options(&handle, PlaybackOptions::new(1.0)),
        Err(AudioError::Paused)
    );
    assert_eq!(mixer.generation, 0);
    assert_eq!(mixer.occupied_voices(), 0);
    mixer.resume(1);
    assert_eq!(mixer.active_voices(), 0);
    let voice = mixer.play(&handle, 1.0).unwrap();
    assert_eq!(voice.generation, 1);
}
