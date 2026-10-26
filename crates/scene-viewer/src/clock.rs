use std::time::{Duration, Instant};

/// Render time for an admitted frame. Fixed time changes only by caller request.
/// Moving fixed time backwards resets shadow-depth evidence and contraction timers.
/// Consumer CPU updates still receive wall-clock `RetainedTick` timing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderClock {
    #[default]
    Wall,
    Fixed(Duration),
}

pub(crate) struct Clock {
    start: Instant,
    selection: RenderClock,
}
impl Clock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            selection: RenderClock::Wall,
        }
    }
    pub fn select(&mut self, selection: RenderClock) {
        self.selection = selection;
    }
    pub fn time(&self) -> Duration {
        match self.selection {
            RenderClock::Wall => self.start.elapsed(),
            RenderClock::Fixed(time) => time,
        }
    }
}
