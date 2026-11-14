use std::{
    hint::black_box,
    time::{Duration, Instant},
};
use worldrunner_world_simulation::audio::{
    AudioMixer, BusConfig, DuckingDetector, DuckingRule, EnvelopeMode,
};

fn constant_clip(
    mixer: &mut AudioMixer,
    frames: usize,
) -> worldrunner_world_simulation::audio::ClipHandle {
    let size = u32::try_from(frames * 4).unwrap();
    let mut wav = b"RIFF".to_vec();
    wav.extend((36 + size).to_le_bytes());
    wav.extend(b"WAVEfmt \x10\0\0\0\x03\0\x01\0");
    wav.extend(48_000_u32.to_le_bytes());
    wav.extend(192_000_u32.to_le_bytes());
    wav.extend([4, 0, 32, 0]);
    wav.extend(b"data");
    wav.extend(size.to_le_bytes());
    for _ in 0..frames {
        wav.extend(0.5_f32.to_le_bytes());
    }
    mixer.load_wav(&wav).unwrap()
}

fn main() {
    let mut mixer = AudioMixer::new(48_000).unwrap();
    let names: Vec<_> = (0..64).map(|i| format!("b{i}")).collect();
    for (i, name) in names.iter().enumerate() {
        mixer
            .configure_bus(
                name,
                BusConfig {
                    gain: 1.0,
                    muted: false,
                    tier: i32::try_from(i).unwrap(),
                },
            )
            .unwrap();
    }
    let rules = (0_u16..64)
        .map(|i| DuckingRule {
            source_buses: names.clone(),
            target_buses: names.clone(),
            gain: 0.25 + f32::from(i) / 256.0,
            attack: Duration::ZERO,
            hold: Duration::ZERO,
            release: Duration::ZERO,
            envelope: EnvelopeMode::LinearGain,
            detector: DuckingDetector::Activity,
        })
        .collect();
    mixer.replace_ducking_rules(rules).unwrap();
    let clip = constant_clip(&mut mixer, 4_800);
    for name in &names[32..] {
        mixer.play_on_bus(&clip, 0.01, name).unwrap();
    }
    let mut output = vec![[0.0; 2]; 4_800];
    let start = Instant::now();
    mixer.render(black_box(&mut output));
    let elapsed = start.elapsed();
    let expected = 0.043_749_996_f32.to_bits();
    assert!(
        black_box(&output)
            .iter()
            .all(|frame| frame.map(f32::to_bits) == [expected; 2])
    );
    println!(
        "64x64x64: frames={} elapsed_ns={} first_sample={}",
        output.len(),
        elapsed.as_nanos(),
        output[0][0]
    );
    // `cargo test --all-targets` also runs this benchmark, without optimization.
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed < Duration::from_millis(100),
        "100 ms of audio took {elapsed:?}"
    );
}
