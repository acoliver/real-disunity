use super::WindowlessRetained;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Progress {
    submitted_frame: u64,
    accepted_generation: u64,
    ready_generation: u64,
    preparations_in_flight: usize,
    capture_delivered: bool,
}

impl Progress {
    fn snapshot(run: &WindowlessRetained<'_>, capture_delivered: bool) -> Self {
        Self {
            submitted_frame: run.session.frame,
            accepted_generation: run.host().accepted_generation(),
            ready_generation: run.host().ready_generation(),
            preparations_in_flight: run.host().preparations_in_flight(),
            capture_delivered,
        }
    }
}

struct ProgressWatchdog {
    awaited: String,
    last_progress: Duration,
    progress: Progress,
}

impl ProgressWatchdog {
    fn new(awaited: String, progress: Progress) -> Self {
        Self {
            awaited,
            last_progress: Duration::ZERO,
            progress,
        }
    }

    fn check(&mut self, elapsed: Duration, progress: Progress) -> Result<(), String> {
        let mut stall = elapsed
            .checked_sub(self.last_progress)
            .expect("monotonic watchdog clock");
        if elapsed >= Duration::from_mins(20) {
            return Err(self.failure("total bound reached", stall, elapsed));
        }
        // Polling alone is not progress; slow work may continue while these signals change.
        if self.progress != progress {
            self.progress = progress;
            self.last_progress = elapsed;
            stall = Duration::ZERO;
        }
        if stall >= Duration::from_mins(3) {
            return Err(self.failure("no observable progress", stall, elapsed));
        }
        Ok(())
    }

    fn failure(&self, reason: &str, stall: Duration, elapsed: Duration) -> String {
        format!(
            "waiting for {}: {reason}; stall {stall:?}, total {elapsed:?}; signals {:?}",
            self.awaited, self.progress
        )
    }
}

pub(super) struct CaptureWatchdog {
    started: Instant,
    progress: ProgressWatchdog,
}

impl CaptureWatchdog {
    pub(super) fn new(run: &WindowlessRetained<'_>, awaited: String) -> Self {
        Self {
            started: Instant::now(),
            progress: ProgressWatchdog::new(awaited, Progress::snapshot(run, false)),
        }
    }

    pub(super) fn check(&mut self, run: &WindowlessRetained<'_>, delivered: bool) {
        self.progress
            .check(self.started.elapsed(), Progress::snapshot(run, delivered))
            .unwrap_or_else(|message| panic!("{message}"));
    }
}

mod tests;
