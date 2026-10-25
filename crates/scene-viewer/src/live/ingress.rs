#[cfg(test)]
mod tests;

use std::{
    collections::VecDeque,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use super::{IngressOrigin, LiveInputEvent, LiveInputSample, ScheduledInput};

const LIMIT: usize = 4096;
const BUDGET_ERROR: &str = "live ingress exceeds 4096 observations before update";

pub(super) struct InputBuffer {
    queued: Vec<(Instant, LiveInputEvent)>,
    schedule: VecDeque<ScheduledInput>,
    origin: Option<Instant>,
    sequence: u64,
    overflowed: bool,
}

impl InputBuffer {
    pub fn new(schedule: Vec<ScheduledInput>) -> Self {
        Self {
            queued: Vec::new(),
            schedule: schedule.into(),
            origin: None,
            sequence: 0,
            overflowed: false,
        }
    }

    pub fn start(&mut self, origin: Instant) {
        self.origin = Some(origin);
    }

    /// Whether any not yet collected OS observation matches, without consuming it.
    pub fn any_queued(&self, matches: impl Fn(&LiveInputEvent) -> bool) -> bool {
        self.queued.iter().any(|(_, event)| matches(event))
    }

    pub fn push(&mut self, event: LiveInputEvent, acquired: Instant) {
        if self.queued.len() == LIMIT {
            self.overflowed = true;
            return;
        }
        self.queued.push((acquired, event));
    }

    pub fn collect(
        &mut self,
        frame: u64,
        replay_step: Duration,
        receiver: Option<&Receiver<LiveInputEvent>>,
    ) -> Result<Vec<LiveInputSample>, String> {
        if self.overflowed {
            return Err(BUDGET_ERROR.into());
        }
        let mut samples = Vec::new();
        while self.schedule.front().is_some_and(|e| e.frame <= frame) {
            let scheduled = self.schedule.pop_front().expect("front exists");
            append(
                &mut samples,
                IngressOrigin::ScheduledReplay,
                replay_timestamp(replay_step, scheduled.frame)?,
                scheduled.event,
            )?;
        }
        let origin = self.origin.expect("input clock starts after initialize");
        for (acquired, event) in self.queued.drain(..) {
            append(
                &mut samples,
                IngressOrigin::OsEventLoop,
                acquired.saturating_duration_since(origin),
                event,
            )?;
        }
        if let Some(receiver) = receiver {
            for event in receiver.try_iter() {
                append(
                    &mut samples,
                    IngressOrigin::ChannelDelivery,
                    Instant::now().saturating_duration_since(origin),
                    event,
                )?;
            }
        }
        let count = u64::try_from(samples.len()).expect("bounded observation count");
        self.sequence
            .checked_add(count)
            .ok_or("live ingress sequence exhausted")?;
        for sample in &mut samples {
            self.sequence += 1;
            sample.sequence = self.sequence;
        }
        Ok(samples)
    }
}

fn append(
    samples: &mut Vec<LiveInputSample>,
    origin: IngressOrigin,
    timestamp: Duration,
    event: LiveInputEvent,
) -> Result<(), String> {
    if samples.len() == LIMIT {
        return Err(BUDGET_ERROR.into());
    }
    samples.push(LiveInputSample {
        sequence: 0,
        timestamp,
        origin,
        event,
    });
    Ok(())
}

fn replay_timestamp(step: Duration, frame: u64) -> Result<Duration, String> {
    let nanos = step
        .as_nanos()
        .checked_mul(u128::from(frame))
        .ok_or("live replay timestamp overflow")?;
    let seconds =
        u64::try_from(nanos / 1_000_000_000).map_err(|_| "live replay timestamp overflow")?;
    Ok(Duration::new(
        seconds,
        u32::try_from(nanos % 1_000_000_000).expect("subsecond nanos"),
    ))
}
