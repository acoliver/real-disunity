//! Opt-in audible device check: cargo run -p worldrunner-scene-viewer --example tone
use std::{error::Error, thread, time::Duration};
use worldrunner_scene_viewer::{AudioService, AudioStatus};

fn main() -> Result<(), Box<dyn Error>> {
    let mut audio = AudioService::default();
    audio.start_device()?;
    let AudioStatus::Ready { sample_rate } = audio.status() else {
        unreachable!("opened device");
    };
    let clip = audio.load_wav(&tone(sample_rate))?;
    audio.play(&clip, 0.15)?;
    thread::sleep(Duration::from_millis(1200));
    if !matches!(audio.status(), AudioStatus::Ready { .. }) {
        return Err(format!("device output failed: {:?}", audio.status()).into());
    }
    Ok(())
}
fn tone(rate: u32) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + rate * 4).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(3_u16.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * 4).to_le_bytes());
    bytes.extend(4_u16.to_le_bytes());
    bytes.extend(32_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((rate * 4).to_le_bytes());
    for index in 0..rate {
        let phase = small_float(index) * 440.0 * std::f32::consts::TAU / small_float(rate);
        bytes.extend(phase.sin().to_le_bytes());
    }
    bytes
}
fn small_float(value: u32) -> f32 {
    f32::from(u16::try_from(value >> 16).expect("high half")) * 65536.0
        + f32::from(u16::try_from(value & 65535).expect("low half"))
}
