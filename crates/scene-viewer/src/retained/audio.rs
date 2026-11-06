//! Device lifetime and host-facing audio. Device startup is explicit and opt-in.
mod device;
mod ring;
#[cfg(test)]
pub(super) mod test_support;
#[cfg(test)]
mod tests;
mod worker;
use super::{RetainedEvent, RetainedEventHost, RetainedHost};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
pub use worldrunner_world_simulation::audio::{
    AudioCommand, AudioError, AudioReply, ClipHandle, VoiceHandle,
};
use worldrunner_world_simulation::audio::{AudioMixer, MAX_VOICES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioUnavailable {
    NotStarted,
    NullDevice,
    NoDevice,
    OpenFailed,
    UnsupportedConfig,
    DeviceLost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioStatus {
    Ready { sample_rate: u32 },
    Unavailable(AudioUnavailable),
    Closed,
}
#[derive(Debug, thiserror::Error)]
pub enum AudioServiceError {
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error("audio device is not available: {0:?}")]
    Unavailable(AudioStatus),
    #[error("audio device startup is only permitted before admitting voices")]
    Busy,
}
struct State {
    mixer: AudioMixer,
    pending: VecDeque<(VoiceHandle, u64)>,
}
impl State {
    fn new() -> Self {
        Self {
            mixer: AudioMixer::new(48000).expect("valid output rate"),
            pending: VecDeque::with_capacity(MAX_VOICES),
        }
    }
}
/// Host calls may lock the mixer. The device callback never accesses this lock;
/// a worker renders into a fixed atomic stereo ring and the callback only reads it.
pub struct AudioService {
    state: Arc<Mutex<State>>,
    status: AudioStatus,
    status_pending: bool,
    detail: Option<String>,
    ring: Option<Arc<ring::Ring>>,
    stream: Option<cpal::Stream>,
    worker: Option<JoinHandle<()>>,
    fractional_frames: u128,
}
impl Default for AudioService {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::new())),
            status: AudioStatus::Unavailable(AudioUnavailable::NotStarted),
            status_pending: false,
            detail: None,
            ring: None,
            stream: None,
            worker: None,
            fractional_frames: 0,
        }
    }
}
impl AudioService {
    pub fn status(&self) -> AudioStatus {
        if self
            .ring
            .as_ref()
            .is_some_and(|ring| ring.lost.load(std::sync::atomic::Ordering::Acquire))
        {
            AudioStatus::Unavailable(AudioUnavailable::DeviceLost)
        } else {
            self.status
        }
    }
    pub fn device_error(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn load_wav(&mut self, bytes: &[u8]) -> Result<ClipHandle, AudioServiceError> {
        if self.status == AudioStatus::Closed {
            return Err(AudioServiceError::Unavailable(self.status));
        }
        Ok(self
            .state
            .lock()
            .expect("audio mixer poisoned")
            .mixer
            .load_wav(bytes)?)
    }
    /// Null-device playback is explicit simulated playback, never a Ready device.
    pub fn play(&mut self, clip: &ClipHandle, gain: f32) -> Result<VoiceHandle, AudioServiceError> {
        if !matches!(
            self.status(),
            AudioStatus::Ready { .. } | AudioStatus::Unavailable(AudioUnavailable::NullDevice)
        ) {
            return Err(AudioServiceError::Unavailable(self.status()));
        }
        let mut state = self.state.lock().expect("audio mixer poisoned");
        if state.mixer.occupied_voices() + state.pending.len() == MAX_VOICES {
            return Err(AudioError::VoiceLimit.into());
        }
        Ok(state.mixer.play(clip, gain)?)
    }
    /// Already queued device samples drain within the 2048-frame handoff bound.
    pub fn stop(&mut self, voice: &VoiceHandle) -> bool {
        let mut state = self.state.lock().expect("audio mixer poisoned");
        if let Some(index) = state.pending.iter().position(|(handle, _)| handle == voice) {
            state.pending.remove(index);
            return true;
        }
        state.mixer.stop(voice)
    }
    pub fn set_master_gain(&mut self, gain: f32) -> Result<(), AudioServiceError> {
        self.require_open()?;
        Ok(self
            .state
            .lock()
            .expect("audio mixer poisoned")
            .mixer
            .set_master_gain(gain)?)
    }
    pub fn set_mute(&mut self, muted: bool) -> Result<(), AudioServiceError> {
        self.require_open()?;
        self.state
            .lock()
            .expect("audio mixer poisoned")
            .mixer
            .set_mute(muted);
        Ok(())
    }
    pub fn command(&mut self, command: AudioCommand) -> Result<AudioReply, AudioServiceError> {
        Ok(match command {
            AudioCommand::Play { clip, gain } => AudioReply::Playing(self.play(&clip, gain)?),
            AudioCommand::Stop(voice) => AudioReply::Stopped(self.stop(&voice)),
            AudioCommand::MasterGain(gain) => {
                self.set_master_gain(gain)?;
                AudioReply::Applied
            }
            AudioCommand::Mute(muted) => {
                self.set_mute(muted)?;
                AudioReply::Applied
            }
        })
    }
    fn require_open(&self) -> Result<(), AudioServiceError> {
        if self.status == AudioStatus::Closed {
            return Err(AudioServiceError::Unavailable(self.status));
        }
        Ok(())
    }
    pub(super) fn null_device(&mut self) {
        self.stop_device();
        self.state
            .lock()
            .expect("audio mixer poisoned")
            .mixer
            .set_output_rate(48000)
            .expect("empty null mixer");
        self.set_status(AudioStatus::Unavailable(AudioUnavailable::NullDevice));
    }
    fn set_status(&mut self, status: AudioStatus) {
        self.status = status;
        self.status_pending = true;
    }
    pub(super) fn advance(&mut self, elapsed: Duration) {
        if self.status != AudioStatus::Unavailable(AudioUnavailable::NullDevice) {
            return;
        }
        let mut state = self.state.lock().expect("audio mixer poisoned");
        let scaled =
            elapsed.as_nanos() * u128::from(state.mixer.output_rate()) + self.fractional_frames;
        let mut frames = scaled / 1_000_000_000;
        self.fractional_frames = scaled % 1_000_000_000;
        let mut output = [[0.0; 2]; ring::BLOCK];
        while frames != 0 && state.mixer.active_voices() != 0 {
            let count = usize::try_from(frames.min(ring::BLOCK as u128)).expect("bounded block");
            state.mixer.render(&mut output[..count]);
            frames -= count as u128;
        }
    }
    pub(super) fn take_event(&mut self) -> Option<RetainedEvent> {
        if self.status() != self.status {
            self.stop_device();
            self.set_status(AudioStatus::Unavailable(AudioUnavailable::DeviceLost));
        }
        if self.status_pending {
            self.status_pending = false;
            return Some(RetainedEvent::AudioStatusChanged {
                status: self.status,
            });
        }
        let mut state = self.state.lock().expect("audio mixer poisoned");
        let voice = if let Some(ring) = &self.ring {
            if state
                .pending
                .front()
                .is_some_and(|(_, end)| *end <= ring.consumed())
            {
                state.pending.pop_front().map(|(voice, _)| voice)
            } else {
                None
            }
        } else {
            state.mixer.take_finished()
        };
        voice.map(|voice| RetainedEvent::AudioFinished { voice })
    }
    pub(super) fn next_wakeup(&self) -> Option<std::time::Instant> {
        if self.status_pending {
            return Some(std::time::Instant::now());
        }
        self.ring
            .as_ref()
            .map(|_| std::time::Instant::now() + Duration::from_millis(5))
    }
    pub(super) fn shutdown(&mut self) {
        self.stop_device();
        self.status = AudioStatus::Closed;
        self.status_pending = false;
    }
    fn stop_device(&mut self) {
        if let Some(ring) = &self.ring {
            ring.running
                .store(false, std::sync::atomic::Ordering::Release);
        }
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            worker.join().expect("audio mixer worker panicked");
        }
        self.ring = None;
        let mut state = self.state.lock().expect("audio mixer poisoned");
        state.mixer.clear_voices();
        state.pending.clear();
    }
}
impl Drop for AudioService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl RetainedHost {
    pub fn audio(&mut self) -> &mut AudioService {
        &mut self.audio
    }
}
impl RetainedEventHost<'_> {
    pub fn audio(&mut self) -> &mut AudioService {
        self.host.audio()
    }
}
