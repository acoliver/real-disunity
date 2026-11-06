use super::{AudioService, AudioServiceError, AudioStatus, AudioUnavailable, ring::Ring, worker};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, atomic::Ordering};

impl AudioService {
    /// Explicitly opens the default device. Never called by constructors or tests.
    /// Windowless null sessions cannot open a physical device.
    pub fn start_device(&mut self) -> Result<(), AudioServiceError> {
        self.start_device_with(Self::open_device)
    }
    pub(in crate::retained) fn start_device_with(
        &mut self,
        open: impl FnOnce(&mut Self) -> Result<(), (AudioUnavailable, String)>,
    ) -> Result<(), AudioServiceError> {
        self.require_open()?;
        if self.status == AudioStatus::Unavailable(AudioUnavailable::NullDevice) {
            return Err(AudioServiceError::Unavailable(self.status));
        }
        let state = self.state.lock().expect("audio mixer poisoned");
        if state.mixer.occupied_voices() + state.pending.len() != 0 {
            return Err(AudioServiceError::Busy);
        }
        drop(state);
        self.stop_device();
        self.detail = None;
        match open(self) {
            Ok(()) => Ok(()),
            Err((reason, detail)) => {
                self.stop_device();
                self.detail = Some(detail);
                self.set_status(AudioStatus::Unavailable(reason));
                Err(AudioServiceError::Unavailable(self.status))
            }
        }
    }
    fn open_device(&mut self) -> Result<(), (AudioUnavailable, String)> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| {
                (
                    AudioUnavailable::NoDevice,
                    "no default output device".into(),
                )
            })?;
        let config = device.default_output_config().map_err(open_error)?;
        if !(1..=2).contains(&config.channels()) {
            return Err((
                AudioUnavailable::UnsupportedConfig,
                "need mono or stereo output".into(),
            ));
        }
        self.state
            .lock()
            .expect("audio mixer poisoned")
            .mixer
            .set_output_rate(config.sample_rate().0)
            .map_err(open_error)?;
        let ring = Ring::new();
        let stream = build(&device, &config, &ring)?;
        stream.play().map_err(open_error)?;
        let worker = worker::start(self.state.clone(), ring.clone()).map_err(open_error)?;
        self.set_status(AudioStatus::Ready {
            sample_rate: config.sample_rate().0,
        });
        self.ring = Some(ring);
        self.stream = Some(stream);
        self.worker = Some(worker);
        Ok(())
    }
}
fn open_error(error: impl std::fmt::Display) -> (AudioUnavailable, String) {
    (AudioUnavailable::OpenFailed, error.to_string())
}
fn build(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    ring: &Arc<Ring>,
) -> Result<cpal::Stream, (AudioUnavailable, String)> {
    match config.sample_format() {
        cpal::SampleFormat::F32 => stream::<f32>(device, &config.config(), ring),
        cpal::SampleFormat::I16 => stream::<i16>(device, &config.config(), ring),
        cpal::SampleFormat::U16 => stream::<u16>(device, &config.config(), ring),
        _ => {
            return Err((
                AudioUnavailable::UnsupportedConfig,
                "need f32, i16 or u16 output".into(),
            ));
        }
    }
    .map_err(open_error)
}
fn stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    ring: &Arc<Ring>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let consumer = ring.clone();
    let errors = ring.clone();
    let channels = usize::from(config.channels);
    device.build_output_stream(
        config,
        move |output: &mut [T], _| consumer.fill(output, channels),
        move |_| {
            errors.lost.store(true, Ordering::Release);
            errors.running.store(false, Ordering::Release);
        },
        None,
    )
}
