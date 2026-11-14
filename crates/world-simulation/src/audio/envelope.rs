use super::{EnvelopeMode, small_float};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Envelope {
    level: f32,
    demand: f32,
    start: f32,
    goal: f32,
    elapsed: u32,
    frames: u32,
    hold: u32,
}

impl Envelope {
    pub fn advance(&mut self, demand: f32, times: [u32; 3]) -> f32 {
        let [attack, hold, release] = times;
        if demand < self.demand {
            self.hold = hold;
            self.goal = self.level;
            self.frames = 0;
        } else if demand > self.demand {
            self.hold = 0;
        }
        self.demand = demand;
        if demand < self.level && self.hold > 0 {
            self.hold -= 1;
            return self.level;
        }
        // A changed sample value starts a new ramp; this is exact input identity.
        if demand.to_bits() != self.goal.to_bits() {
            self.start = self.level;
            self.goal = demand;
            self.elapsed = 0;
            self.frames = if demand > self.level { attack } else { release };
        }
        if self.frames == 0 || self.elapsed >= self.frames {
            self.level = self.goal;
        } else {
            self.elapsed += 1;
            self.level = if self.elapsed == self.frames {
                self.goal
            } else {
                self.start
                    + (self.goal - self.start) * small_float(self.elapsed)
                        / small_float(self.frames)
            };
        }
        self.level
    }
}

pub(super) fn duration_frames(duration: Duration, rate: u32) -> u32 {
    u32::try_from((duration.as_nanos() * u128::from(rate)).div_ceil(1_000_000_000))
        .expect("validated ducking time and rate")
}

pub(super) fn gain_at(mode: EnvelopeMode, gain: f32, depth: f32) -> f32 {
    if depth == 0.0 {
        return 1.0;
    }
    if depth >= 1.0 {
        return gain;
    }
    match mode {
        EnvelopeMode::LinearGain => 1.0 + (gain - 1.0) * depth,
        EnvelopeMode::LinearDb => gain.powf(depth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_demand_falling_to_current_depth_stays_there() {
        let mut envelope = Envelope::default();
        assert_eq!(
            envelope.advance(1.0, [4, 2, 4]).to_bits(),
            0.25_f32.to_bits()
        );
        assert_eq!(
            envelope.advance(1.0, [4, 2, 4]).to_bits(),
            0.5_f32.to_bits()
        );
        assert_eq!(
            envelope.advance(0.5, [4, 2, 4]).to_bits(),
            0.5_f32.to_bits()
        );
        assert_eq!(
            envelope.advance(0.5, [4, 2, 4]).to_bits(),
            0.5_f32.to_bits()
        );
    }
}
