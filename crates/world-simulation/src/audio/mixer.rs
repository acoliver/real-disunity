use super::{
    AudioCommand, AudioError, AudioReply, Clip, ClipHandle, MAX_CLIPS, MAX_DECODED_BYTES,
    MAX_VOICES, Owner, VoiceHandle, small_float, validate_gain, validate_rate,
};
use std::{collections::VecDeque, sync::Arc};

struct Voice {
    handle: VoiceHandle,
    clip: Clip,
    position: u64,
    gain: f32,
}

/// Linear interpolation with a held final sample, integer rational phase and
/// hard clipping after summing all voices. Mute advances time normally.
/// Render writes caller storage without allocating. Completions occupy voice
/// capacity until drained, so neither render nor an unattended consumer can grow it.
pub struct AudioMixer {
    owner: Owner,
    rate: u32,
    clips: Vec<Clip>,
    decoded_bytes: usize,
    voices: Vec<Voice>,
    finished: VecDeque<VoiceHandle>,
    generation: u64,
    master: f32,
    muted: bool,
}
impl AudioMixer {
    pub fn new(output_rate: u32) -> Result<Self, AudioError> {
        validate_rate(output_rate)?;
        Ok(Self {
            owner: Owner(Arc::new(())),
            rate: output_rate,
            clips: Vec::new(),
            decoded_bytes: 0,
            voices: Vec::with_capacity(MAX_VOICES),
            finished: VecDeque::with_capacity(MAX_VOICES),
            generation: 0,
            master: 1.0,
            muted: false,
        })
    }
    pub fn output_rate(&self) -> u32 {
        self.rate
    }
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }
    /// Output reconfiguration is only valid before admitting voices.
    pub fn set_output_rate(&mut self, rate: u32) -> Result<(), AudioError> {
        validate_rate(rate)?;
        if self.occupied_voices() != 0 {
            return Err(AudioError::OutputBusy);
        }
        self.rate = rate;
        Ok(())
    }
    pub fn occupied_voices(&self) -> usize {
        self.voices.len() + self.finished.len()
    }
    pub fn load_wav(&mut self, bytes: &[u8]) -> Result<ClipHandle, AudioError> {
        if self.clips.len() == MAX_CLIPS {
            return Err(AudioError::ClipLimit);
        }
        let clip = Clip::from_wav(bytes)?;
        let size = std::mem::size_of_val(clip.samples());
        if self.decoded_bytes + size > MAX_DECODED_BYTES {
            return Err(AudioError::ClipLimit);
        }
        let handle = ClipHandle {
            owner: self.owner.clone(),
            index: self.clips.len(),
        };
        self.clips.push(clip);
        self.decoded_bytes += size;
        Ok(handle)
    }
    pub fn play(&mut self, clip: &ClipHandle, gain: f32) -> Result<VoiceHandle, AudioError> {
        validate_gain(gain)?;
        if clip.owner != self.owner {
            return Err(AudioError::UnknownClip);
        }
        if self.occupied_voices() == MAX_VOICES {
            return Err(AudioError::VoiceLimit);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("audio voice identity exhausted");
        let handle = VoiceHandle {
            owner: self.owner.clone(),
            generation: self.generation,
        };
        self.voices.push(Voice {
            handle: handle.clone(),
            clip: self.clips[clip.index].clone(),
            position: 0,
            gain,
        });
        Ok(handle)
    }
    /// Explicit stop produces no natural-completion event.
    pub fn stop(&mut self, handle: &VoiceHandle) -> bool {
        let Some(index) = self.voices.iter().position(|voice| &voice.handle == handle) else {
            return false;
        };
        self.voices.remove(index);
        true
    }
    pub fn set_master_gain(&mut self, gain: f32) -> Result<(), AudioError> {
        validate_gain(gain)?;
        self.master = gain;
        Ok(())
    }
    pub fn set_mute(&mut self, muted: bool) {
        self.muted = muted;
    }
    pub fn command(&mut self, command: AudioCommand) -> Result<AudioReply, AudioError> {
        Ok(match command {
            AudioCommand::Play { clip, gain } => AudioReply::Playing(self.play(&clip, gain)?),
            AudioCommand::Stop(voice) => AudioReply::Stopped(self.stop(&voice)),
            AudioCommand::MasterGain(gain) => {
                self.set_master_gain(gain)?;
                AudioReply::Applied
            }
            AudioCommand::Mute(muted) => {
                self.set_mute(muted);
                AudioReply::Applied
            }
        })
    }
    pub fn take_finished(&mut self) -> Option<VoiceHandle> {
        self.finished.pop_front()
    }
    pub fn clear_voices(&mut self) {
        self.voices.clear();
        self.finished.clear();
    }
    pub fn render(&mut self, output: &mut [[f32; 2]]) {
        for frame in output {
            *frame = [0.0; 2];
            for voice in &mut self.voices {
                add_voice(frame, voice, self.rate);
            }
            for sample in frame {
                *sample = if self.muted {
                    0.0
                } else {
                    (*sample * self.master).clamp(-1.0, 1.0)
                };
            }
            let mut index = 0;
            while index < self.voices.len() {
                let voice = &self.voices[index];
                if voice.position >= voice.clip.samples().len() as u64 * u64::from(self.rate) {
                    self.finished.push_back(self.voices.remove(index).handle);
                } else {
                    index += 1;
                }
            }
        }
    }
}
fn add_voice(frame: &mut [f32; 2], voice: &mut Voice, rate: u32) {
    let index = usize::try_from(voice.position / u64::from(rate)).expect("bounded clip position");
    let samples = voice.clip.samples();
    let next = (index + 1).min(samples.len() - 1);
    let fraction =
        small_float(u32::try_from(voice.position % u64::from(rate)).expect("rate remainder"))
            / small_float(rate);
    for channel in 0..2 {
        frame[channel] += (samples[index][channel]
            + (samples[next][channel] - samples[index][channel]) * fraction)
            * voice.gain;
    }
    voice.position += u64::from(voice.clip.sample_rate());
}
