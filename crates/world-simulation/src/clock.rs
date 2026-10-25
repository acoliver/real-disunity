use crate::{TimeError, TimeSpan};

#[derive(Clone, Copy)]
pub(crate) struct FixedPhase {
    pub time: TimeSpan,
    pub frame: u64,
    pub cutoff: TimeSpan,
    pub unscaled_delta: TimeSpan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clock {
    pub time: TimeSpan,
    pub fixed_time: TimeSpan,
    pub unscaled_time: TimeSpan,
    pub delta: TimeSpan,
    pub unscaled_delta: TimeSpan,
    pub frame: u64,
    pub fixed_frame: u64,
    pub fixed_step: TimeSpan,
    pub time_scale: TimeSpan,
    pub(crate) accumulator: TimeSpan,
    pub(crate) max_steps: u32,
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            time: TimeSpan::ZERO,
            fixed_time: TimeSpan::ZERO,
            unscaled_time: TimeSpan::ZERO,
            delta: TimeSpan::ZERO,
            unscaled_delta: TimeSpan::ZERO,
            frame: 0,
            fixed_frame: 0,
            fixed_step: TimeSpan::FIXED,
            time_scale: TimeSpan::ONE,
            accumulator: TimeSpan::ZERO,
            max_steps: 1024,
        }
    }
}
impl Clock {
    /// Stage all arithmetic and cutoffs before publishing any frame state.
    pub(crate) fn plan(&self, elapsed: TimeSpan) -> Result<(Self, Vec<FixedPhase>), TimeError> {
        let scaled = elapsed.checked_mul(self.time_scale)?;
        let accumulated = self.accumulator.checked_add(scaled)?;
        let steps = if scaled == TimeSpan::ZERO {
            0
        } else {
            accumulated.steps(self.fixed_step)
        };
        if steps > u128::from(self.max_steps) {
            return Err(TimeError::StepBudget);
        }
        let steps = u32::try_from(steps).map_err(|_| TimeError::Overflow)?;
        let unscaled_delta = if steps == 0 {
            TimeSpan::ZERO
        } else {
            self.fixed_step.checked_div(self.time_scale)?
        };
        let mut next = self.clone();
        next.frame = self.frame.checked_add(1).ok_or(TimeError::Overflow)?;
        next.time = self.time.checked_add(scaled)?;
        next.unscaled_time = self.unscaled_time.checked_add(elapsed)?;
        next.delta = scaled;
        next.unscaled_delta = elapsed;
        let mut phases = Vec::new();
        let mut fixed_time = self.fixed_time;
        let mut fixed_frame = self.fixed_frame;
        for step in 1..=steps {
            fixed_time = fixed_time.checked_add(self.fixed_step)?;
            fixed_frame = fixed_frame.checked_add(1).ok_or(TimeError::Overflow)?;
            let offset = self
                .fixed_step
                .checked_mul(TimeSpan::new(u64::from(step), 1)?)?
                .checked_sub(self.accumulator)?
                .checked_div(self.time_scale)?;
            phases.push(FixedPhase {
                time: fixed_time,
                frame: fixed_frame,
                cutoff: self.unscaled_time.checked_add(offset)?,
                unscaled_delta,
            });
        }
        next.accumulator = accumulated.checked_sub(
            self.fixed_step
                .checked_mul(TimeSpan::new(u64::from(steps), 1)?)?,
        )?;
        Ok((next, phases))
    }
}
