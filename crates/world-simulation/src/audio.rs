//! Bounded short-clip state and deterministic device-independent stereo mixing.
mod buses;
mod envelope;
mod mixer;
mod wav;
pub use buses::{
    BusConfig, DuckingDetector, DuckingRule, EnvelopeMode, MAX_AUDIO_BUSES, MAX_BUS_NAME_BYTES,
    MAX_DUCKING_RULES, MAX_DUCKING_TIME,
};
pub use mixer::AudioMixer;
use std::sync::Arc;
pub use wav::Clip;

pub const MAX_WAV_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CLIPS: usize = 64;
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_VOICES: usize = 32;
pub const MAX_SAMPLE_RATE: u32 = 384_000;

#[derive(Clone, Debug)]
struct Owner(Arc<()>);
impl PartialEq for Owner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Owner {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipHandle {
    owner: Owner,
    index: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceHandle {
    owner: Owner,
    generation: u64,
}

/// Whether a pause call also suspends currently eligible voices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExistingVoices {
    Continue,
    Suspend,
}

/// Admission and routing for one voice, independent of bus gain and clocks.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackOptions {
    pub gain: f32,
    pub bus: Option<String>,
    pub pause_eligible: bool,
}
impl PlaybackOptions {
    /// Ordinary playback is unrouted and pause-eligible.
    pub fn new(gain: f32) -> Self {
        Self {
            gain,
            bus: None,
            pause_eligible: true,
        }
    }
}

/// Typed controls; clip loading accepts a bounded borrowed byte slice separately.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioCommand {
    Play {
        clip: ClipHandle,
        gain: f32,
    },
    PlayOnBus {
        clip: ClipHandle,
        gain: f32,
        bus: String,
    },
    PlayWithOptions {
        clip: ClipHandle,
        options: PlaybackOptions,
    },
    PauseMask,
    Pause {
        bits: u32,
        existing: ExistingVoices,
    },
    Resume {
        bits: u32,
    },
    ConfigureBus {
        name: String,
        config: BusConfig,
    },
    DuckingRules(Vec<DuckingRule>),
    Stop(VoiceHandle),
    MasterGain(f32),
    Mute(bool),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioReply {
    Playing(VoiceHandle),
    Stopped(bool),
    PauseMask(u32),
    Applied,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AudioError {
    #[error("unknown audio bus: {0}")]
    UnknownBus(String),
    #[error("audio bus name must contain 1..=128 bytes: {0}")]
    InvalidBusName(String),
    #[error("audio bus limit reached (64)")]
    BusLimit,
    #[error("ducking rule limit reached (64)")]
    DuckingRuleLimit,
    #[error(
        "invalid ducking rule: need nonempty bus sets, gain in 0..=1 (positive for linear-db), and times at most 3600 seconds"
    )]
    InvalidDuckingRule,
    #[error("unsupported ducking envelope mode: {0}")]
    UnsupportedEnvelope(String),
    #[error("WAV input exceeds the 8 MiB short-clip limit")]
    ClipTooLarge,
    #[error("malformed or truncated RIFF/WAVE file")]
    MalformedWav,
    #[error("unsupported WAV encoding: need PCM 8/16/24 or float32, mono/stereo")]
    UnsupportedFormat,
    #[error("sample rate must be in 1..=384000 Hz")]
    InvalidSampleRate,
    #[error("WAV contains a nonfinite sample")]
    InvalidSample,
    #[error("clip count or decoded-memory limit reached")]
    ClipLimit,
    #[error("clip handle belongs to another mixer")]
    UnknownClip,
    #[error("simultaneous voice/completion limit reached (32)")]
    VoiceLimit,
    #[error("gain must be finite and in 0..=16")]
    InvalidGain,
    #[error("output reconfiguration requires empty voice/completion slots")]
    OutputBusy,
    #[error("pause-eligible playback is paused")]
    Paused,
}

fn validate_gain(gain: f32) -> Result<(), AudioError> {
    if !gain.is_finite() || !(0.0..=16.0).contains(&gain) {
        return Err(AudioError::InvalidGain);
    }
    Ok(())
}
fn validate_rate(rate: u32) -> Result<(), AudioError> {
    if !(1..=MAX_SAMPLE_RATE).contains(&rate) {
        return Err(AudioError::InvalidSampleRate);
    }
    Ok(())
}

fn small_float(value: u32) -> f32 {
    f32::from(u16::try_from(value >> 16).expect("high half")) * 65536.0
        + f32::from(u16::try_from(value & 65535).expect("low half"))
}
