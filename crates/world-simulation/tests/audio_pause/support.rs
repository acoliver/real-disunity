use worldrunner_world_simulation::audio::{AudioMixer, BusConfig, ClipHandle, PlaybackOptions};

pub fn clip(mixer: &mut AudioMixer, samples: &[f32], rate: u32) -> ClipHandle {
    let data: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
    let size = u32::try_from(data.len()).unwrap();
    let mut wav = b"RIFF".to_vec();
    wav.extend((36 + size).to_le_bytes());
    wav.extend(b"WAVEfmt \x10\0\0\0\x03\0\x01\0");
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend([4, 0, 32, 0]);
    wav.extend(b"data");
    wav.extend(size.to_le_bytes());
    wav.extend(data);
    mixer.load_wav(&wav).unwrap()
}

pub fn bus(mixer: &mut AudioMixer, name: &str, gain: f32, muted: bool, tier: i32) {
    mixer
        .configure_bus(name, BusConfig { gain, muted, tier })
        .unwrap();
}

pub fn options(bus: Option<&str>, pause_eligible: bool) -> PlaybackOptions {
    PlaybackOptions {
        bus: bus.map(str::to_owned),
        pause_eligible,
        ..PlaybackOptions::new(1.0)
    }
}

pub fn render(mixer: &mut AudioMixer, count: usize) -> Vec<[f32; 2]> {
    let mut out = vec![[0.0; 2]; count];
    mixer.render(&mut out);
    out
}
