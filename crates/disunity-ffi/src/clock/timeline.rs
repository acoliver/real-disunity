use crate::ratio::Ratio;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub(crate) struct Timeline {
    pub time: Ratio,
    pub unscaled_time: Ratio,
    pub delta_time: Ratio,
    pub unscaled_delta_time: Ratio,
    pub fixed_delta_time: Ratio,
    pub time_scale: Ratio,
    pub accumulator: Ratio,
    pub admitted_fixed_time: Ratio,
}
impl Default for Timeline {
    fn default() -> Self {
        Self {
            time: Ratio::ZERO,
            unscaled_time: Ratio::ZERO,
            delta_time: Ratio::ZERO,
            unscaled_delta_time: Ratio::ZERO,
            fixed_delta_time: Ratio::FIXED,
            time_scale: Ratio::ONE,
            accumulator: Ratio::ZERO,
            admitted_fixed_time: Ratio::ZERO,
        }
    }
}
impl Timeline {
    pub fn configure(&mut self, scale: Option<Ratio>, fixed: Option<Ratio>) -> Result<(), String> {
        if let Some(scale) = scale {
            let scale = scale.checked()?;
            if scale > (Ratio { num: 100, den: 1 }) {
                return Err("time_scale must be in 0..100".into());
            }
            self.time_scale = scale;
        }
        if let Some(fixed) = fixed {
            let fixed = fixed.checked()?;
            if fixed == Ratio::ZERO || fixed > Ratio::ONE {
                return Err("fixed_delta_time must be in (0,1] seconds".into());
            }
            self.fixed_delta_time = fixed;
        }
        Ok(())
    }
    pub fn begin(&mut self, dt: Ratio) -> Result<u32, String> {
        let dt = dt.checked()?;
        if dt > Ratio::ONE {
            return Err("dt must be in 0..1 seconds".into());
        }
        let scaled = dt.mul(self.time_scale)?;
        let accumulated = self.accumulator.add(scaled)?;
        let steps = if scaled == Ratio::ZERO {
            0
        } else {
            accumulated.steps(self.fixed_delta_time)
        };
        if steps > 1024 {
            return Err("fixed-step admission exceeds 1024 steps per frame".into());
        }
        let admitted = self.fixed_delta_time.mul(Ratio {
            num: steps as u64,
            den: 1,
        })?;
        self.time = self.time.add(scaled)?;
        self.unscaled_time = self.unscaled_time.add(dt)?;
        self.accumulator = accumulated.sub(admitted)?;
        self.admitted_fixed_time = self.admitted_fixed_time.add(admitted)?;
        self.delta_time = scaled;
        self.unscaled_delta_time = dt;
        Ok(steps as u32)
    }
}
