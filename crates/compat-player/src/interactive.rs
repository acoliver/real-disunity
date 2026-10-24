use compat_kit::RunRequest;
use serde::Deserialize;
use std::time::Duration;
use worldrunner::scene::viewer::LiveTick;

const MAX_STEP: f64 = 1.0 / 60.0;
const MAX_CATCH_UP: f64 = 0.25;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub duration_ms: u64,
    pub max_frames: u64,
    pub max_captures: u64,
}

impl Config {
    pub fn admit(request: &RunRequest) -> Result<Option<Self>, String> {
        let Some(value) = request.metadata.get("interactive") else {
            return Ok(None);
        };
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid interactive configuration: {error}"))?;
        config.validate()?;
        if !request.desktop {
            return Err("interactive requires desktop execution".into());
        }
        if request.metadata.get("assembly").is_some()
            || request.metadata.get("assembly_qualification").is_some()
        {
            return Err("interactive requires fresh original-source compilation".into());
        }
        if request.timeline.len() != 1
            || request.timeline[0]["action"] != "load"
            || request.timeline[0]["frame"].as_u64() != Some(0)
        {
            return Err("interactive timeline must contain only one frame-zero load".into());
        }
        Ok(Some(config))
    }

    fn validate(self) -> Result<(), String> {
        if !(1..=60_000).contains(&self.duration_ms)
            || !(1..=36_000).contains(&self.max_frames)
            || !(1..=64).contains(&self.max_captures)
        {
            return Err("interactive budgets require duration_ms 1..60000, max_frames 1..36000, max_captures 1..64".into());
        }
        Ok(())
    }
}

pub(crate) struct Continuous {
    pub config: Config,
    simulated: f64,
    pub captures: u64,
}

impl Continuous {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            simulated: 0.0,
            captures: 0,
        }
    }

    pub fn finished(&self, tick: &LiveTick) -> bool {
        tick.frame.saturating_add(1) >= self.config.max_frames
            || tick.elapsed.saturating_add(tick.dt)
                >= Duration::from_millis(self.config.duration_ms)
    }

    pub fn steps(&mut self, tick: &LiveTick) -> Vec<f64> {
        // Bound stall recovery; the wall-clock deadline still includes discarded time.
        let remaining = Duration::from_millis(self.config.duration_ms)
            .saturating_sub(tick.elapsed)
            .as_secs_f64();
        let mut delta = tick.dt.as_secs_f64().min(MAX_CATCH_UP).min(remaining);
        let mut steps = Vec::new();
        for _ in 0..15 {
            if delta <= 0.0 {
                break;
            }
            let step = delta.min(MAX_STEP);
            steps.push(step);
            self.simulated += step;
            delta -= step;
        }
        steps
    }

    pub fn simulated(&self) -> f64 {
        self.simulated
    }
}

#[cfg(test)]
#[path = "interactive_tests.rs"]
mod tests;
