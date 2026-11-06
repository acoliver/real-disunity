//! Single producer and single consumer, with atomic sample storage (no unsafe).
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
pub(super) const CAPACITY: usize = 2048;
pub(super) const BLOCK: usize = 128;

pub(super) struct Ring {
    samples: Box<[[AtomicU32; 2]]>,
    written: AtomicU64,
    consumed: AtomicU64,
    pub running: AtomicBool,
    pub lost: AtomicBool,
}
impl Ring {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            samples: (0..CAPACITY)
                .map(|_| std::array::from_fn(|_| AtomicU32::new(0)))
                .collect(),
            written: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
            running: AtomicBool::new(true),
            lost: AtomicBool::new(false),
        })
    }
    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::Acquire)
    }
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Acquire)
    }
    pub fn free(&self) -> usize {
        CAPACITY - usize::try_from(self.written() - self.consumed()).expect("bounded queue")
    }
    /// Called only by the mixer worker, with at most `free()` frames.
    pub fn push(&self, frames: &[[f32; 2]]) -> u64 {
        let written = self.written();
        for (offset, frame) in frames.iter().enumerate() {
            let index =
                usize::try_from((written + offset as u64) % CAPACITY as u64).expect("ring index");
            for (target, sample) in self.samples[index].iter().zip(frame) {
                target.store(sample.to_bits(), Ordering::Relaxed);
            }
        }
        let end = written + frames.len() as u64;
        self.written.store(end, Ordering::Release);
        end
    }
    /// Callback path: no allocation, deallocation, locking, logging or user hooks.
    /// Only the cpal callback owns the consumer side. Underruns produce silence
    /// without advancing the consumed cursor or completing unplayed voices.
    pub fn fill<T: cpal::Sample + cpal::FromSample<f32>>(&self, output: &mut [T], channels: usize) {
        let mut cursor = self.consumed.load(Ordering::Relaxed);
        let end = self.written.load(Ordering::Acquire);
        for frame in output.chunks_exact_mut(channels) {
            let stereo = if cursor < end && self.running.load(Ordering::Relaxed) {
                let index = usize::try_from(cursor % CAPACITY as u64).expect("ring index");
                cursor += 1;
                self.samples[index]
                    .each_ref()
                    .map(|sample| f32::from_bits(sample.load(Ordering::Relaxed)))
            } else {
                [0.0; 2]
            };
            if channels == 1 {
                frame[0] = T::from_sample((stereo[0] + stereo[1]) * 0.5);
            } else {
                for (target, sample) in frame.iter_mut().zip(stereo) {
                    *target = T::from_sample(sample);
                }
            }
        }
        self.consumed.store(cursor, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_consumes_preallocated_storage_wraps_and_silences_underruns() {
        let ring = Ring::new();
        let pointer = ring.samples.as_ptr();
        let input = [[0.25, -0.5]; BLOCK];
        for _ in 0..40 {
            assert_eq!(ring.free(), CAPACITY);
            ring.push(&input);
            let mut output = [1_f32; BLOCK * 2 + 2];
            ring.fill(&mut output, 2);
            assert_eq!(&output[..4], &[0.25, -0.5, 0.25, -0.5]);
            assert_eq!(&output[BLOCK * 2..], &[0.0, 0.0]);
            assert_eq!(ring.samples.as_ptr(), pointer);
            assert_eq!(ring.free(), CAPACITY);
        }
    }
    #[test]
    fn callback_converts_mono_and_integer_samples_and_stops() {
        let ring = Ring::new();
        ring.push(&[[0.5, -0.5], [1.0, 1.0]]);
        let mut output = [0_i16; 2];
        ring.fill(&mut output, 1);
        assert_eq!(output, [0, i16::MAX]);
        ring.push(&[[1.0; 2]]);
        ring.running.store(false, Ordering::Relaxed);
        let mut output = [1_f32; 2];
        ring.fill(&mut output, 2);
        assert_eq!(output.map(f32::to_bits), [0.0_f32; 2].map(f32::to_bits));
        assert_eq!(ring.consumed(), 2);
    }
}
