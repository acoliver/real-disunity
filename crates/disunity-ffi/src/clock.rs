use crate::{input::validate_dt, ratio::Ratio};
use serde::Serialize;
use serde_json::{Value, json};
mod timeline;

#[derive(Clone, Serialize)]
pub(crate) struct Clock {
    pub time: f64,
    pub unscaled_time: f64,
    pub unscaled_delta_time: f32,
    pub fixed_unscaled_delta_time: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rational: Option<timeline::Timeline>,
    #[serde(skip)]
    frame_unscaled_delta_time: f32,
    pub delta_time: f32,
    pub fixed_delta_time: f32,
    pub frame: u64,
    pub fixed_frame: u64,
    pub time_scale: f32,
    pub fixed_steps: u32,
    #[serde(skip)]
    accumulator: f64,
    #[serde(skip)]
    frame_delta_time: f32,
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            time: 0.0,
            unscaled_time: 0.0,
            unscaled_delta_time: 0.0,
            fixed_unscaled_delta_time: 0.02,
            rational: None,
            frame_unscaled_delta_time: 0.0,
            delta_time: 0.0,
            fixed_delta_time: 0.02,
            frame: 0,
            fixed_frame: 0,
            time_scale: 1.0,
            fixed_steps: 0,
            accumulator: 0.0,
            frame_delta_time: 0.0,
        }
    }
}
impl Clock {
    pub fn begin(&mut self, dt: f32) -> Result<Value, String> {
        self.require_legacy()?;
        validate_dt(dt)?;
        let scaled = dt * self.time_scale;
        let accumulated = self.accumulator + f64::from(scaled);
        let fixed = f64::from(self.fixed_delta_time);
        let steps = if scaled == 0.0 {
            0.0
        } else {
            (accumulated / fixed + 1e-6).floor()
        };
        if steps > 1024.0 {
            return Err("fixed-step admission exceeds 1024 steps per frame".into());
        }
        self.accumulator = (accumulated - steps * fixed).max(0.0);
        self.fixed_steps = steps as u32;
        self.advance(scaled, dt)?;
        Ok(json!(self))
    }
    fn advance(&mut self, scaled: f32, dt: f32) -> Result<(), String> {
        let frame = self.frame.checked_add(1).ok_or("frame overflow")?;
        self.unscaled_time += f64::from(dt);
        self.unscaled_delta_time = dt;
        self.frame_unscaled_delta_time = dt;
        self.delta_time = scaled;
        self.frame_delta_time = scaled;
        self.time += f64::from(scaled);
        self.frame = frame;
        Ok(())
    }
    pub fn begin_legacy(&mut self, dt: f32) -> Result<Value, String> {
        self.require_legacy()?;
        validate_dt(dt)?;
        self.fixed_steps = 0;
        self.advance(dt * self.time_scale, dt)?;
        Ok(json!(self))
    }
    pub fn phase(&mut self, phase: &str) -> Result<Value, String> {
        self.unscaled_delta_time = match phase {
            "fixed" => self.fixed_unscaled_delta_time,
            "update" => self.frame_unscaled_delta_time,
            _ => return Err("phase must be fixed or update".into()),
        };
        self.delta_time = match phase {
            "fixed" => self.fixed_delta_time,
            "update" => self.frame_delta_time,
            _ => return Err("phase must be fixed or update".into()),
        };
        Ok(json!(self))
    }
    pub fn set(
        &mut self,
        time_scale: Option<f32>,
        fixed_delta_time: Option<f32>,
    ) -> Result<Value, String> {
        self.require_legacy()?;
        if time_scale.is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v)) {
            return Err("time_scale must be finite in 0..100".into());
        }
        if let Some(dt) = fixed_delta_time {
            validate_dt(dt)?;
            if dt == 0.0 {
                return Err("fixed_delta_time must be positive".into());
            }
        }
        if let Some(v) = time_scale {
            self.time_scale = v;
        }
        if let Some(v) = fixed_delta_time {
            self.fixed_delta_time = v;
        }
        self.refresh_fixed_unscaled();
        Ok(json!(self))
    }
}

impl Clock {
    pub fn require_legacy(&self) -> Result<(), String> {
        if self.rational.is_some() {
            return Err("rational clock requires rational frame/configuration operations".into());
        }
        Ok(())
    }
    pub fn endpoint(&self) -> Result<Ratio, String> {
        self.rational
            .as_ref()
            .map(|t| t.unscaled_time)
            .ok_or_else(|| "timestamped input requires rational clock selection".into())
    }
    fn exact(&self) -> Result<timeline::Timeline, String> {
        if let Some(timeline) = &self.rational {
            return Ok(timeline.clone());
        }
        if self.frame != 0 || self.fixed_frame != 0 {
            return Err("rational clock must be selected before legacy advancement".into());
        }
        Ok(timeline::Timeline::default())
    }
    pub fn set_rational(
        &mut self,
        scale: Option<Ratio>,
        fixed: Option<Ratio>,
    ) -> Result<Value, String> {
        let mut timeline = self.exact()?;
        timeline.configure(scale, fixed)?;
        self.project_exact(timeline);
        Ok(json!(self))
    }
    pub fn begin_rational(&mut self, dt: Ratio) -> Result<Value, String> {
        let mut timeline = self.exact()?;
        let frame = self.frame.checked_add(1).ok_or("frame overflow")?;
        let steps = timeline.begin(dt)?;
        self.project_exact(timeline);
        self.frame = frame;
        self.fixed_steps = steps;
        Ok(json!(self))
    }
    fn project_exact(&mut self, timeline: timeline::Timeline) {
        self.time = timeline.time.float();
        self.unscaled_time = timeline.unscaled_time.float();
        self.delta_time = timeline.delta_time.float() as f32;
        self.frame_delta_time = self.delta_time;
        self.unscaled_delta_time = timeline.unscaled_delta_time.float() as f32;
        self.frame_unscaled_delta_time = self.unscaled_delta_time;
        self.fixed_delta_time = timeline.fixed_delta_time.float() as f32;
        self.time_scale = timeline.time_scale.float() as f32;
        self.rational = Some(timeline);
        self.refresh_fixed_unscaled();
    }
    fn refresh_fixed_unscaled(&mut self) {
        self.fixed_unscaled_delta_time = if self.time_scale == 0.0 {
            0.0
        } else {
            self.fixed_delta_time / self.time_scale
        };
    }
}
