//! Event-thread timers on the host monotonic clock. No worker or GPU is involved.
use super::{RetainedEventHost, RetainedHost};
use std::{
    collections::BTreeMap,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

/// Maximum live timers per host, including repeating timers awaiting their next expiry.
pub const MAX_HOST_TIMERS: usize = 256;

/// Host-scoped identity. Obsolete or foreign handles cannot cancel another timer.
#[derive(Clone, Debug)]
pub struct TimerHandle {
    generation: u64,
    owner: Weak<()>,
}
impl PartialEq for TimerHandle {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && self.owner.ptr_eq(&other.owner)
    }
}
impl Eq for TimerHandle {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TimerError {
    #[error("host timer limit exceeded ({limit})")]
    Full { limit: usize },
    #[error("repeating timer interval must be positive")]
    ZeroInterval,
    #[error("timer deadline exceeds the monotonic clock range")]
    DeadlineOverflow,
    #[error("host timer service is closed")]
    Closed,
}

#[derive(Default)]
enum Clock {
    #[default]
    Wall,
    Manual(Instant),
}
struct Entry {
    deadline: Instant,
    interval: Option<Duration>,
}
#[derive(Default)]
pub(super) struct Timers {
    clock: Clock,
    owner: Rc<()>,
    generation: u64,
    live: BTreeMap<u64, Entry>,
    closed: bool,
}
impl Timers {
    fn now(&self) -> Instant {
        match self.clock {
            Clock::Wall => Instant::now(),
            Clock::Manual(now) => now,
        }
    }
    pub(super) fn freeze(&mut self) {
        self.clock = Clock::Manual(self.now());
    }
    pub(super) fn advance(&mut self, elapsed: Duration) -> Result<(), TimerError> {
        let Clock::Manual(now) = &mut self.clock else {
            unreachable!("only windowless sessions advance the clock explicitly");
        };
        *now = now
            .checked_add(elapsed)
            .ok_or(TimerError::DeadlineOverflow)?;
        Ok(())
    }
    fn insert(
        &mut self,
        deadline: Instant,
        interval: Option<Duration>,
    ) -> Result<TimerHandle, TimerError> {
        if self.closed {
            return Err(TimerError::Closed);
        }
        if self.live.len() == MAX_HOST_TIMERS {
            return Err(TimerError::Full {
                limit: MAX_HOST_TIMERS,
            });
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("timer generation exhausted");
        self.live
            .insert(self.generation, Entry { deadline, interval });
        Ok(self.handle(self.generation))
    }
    fn handle(&self, generation: u64) -> TimerHandle {
        TimerHandle {
            generation,
            owner: Rc::downgrade(&self.owner),
        }
    }
    fn cancel(&mut self, handle: &TimerHandle) -> bool {
        handle.owner.ptr_eq(&Rc::downgrade(&self.owner))
            && self.live.remove(&handle.generation).is_some()
    }
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.live.values().map(|entry| entry.deadline).min()
    }
    pub(super) fn due(&self, now: Instant) -> Vec<u64> {
        let mut due: Vec<_> = self
            .live
            .iter()
            .filter(|(_, entry)| entry.deadline <= now)
            .map(|(&id, entry)| (entry.deadline, id))
            .collect();
        due.sort_unstable();
        due.into_iter().map(|(_, id)| id).collect()
    }
    pub(super) fn fire(
        &mut self,
        generation: u64,
        now: Instant,
    ) -> Result<Option<TimerHandle>, TimerError> {
        let Some(entry) = self.live.get_mut(&generation) else {
            return Ok(None);
        };
        if let Some(interval) = entry.interval {
            entry.deadline = now
                .checked_add(interval)
                .ok_or(TimerError::DeadlineOverflow)?;
        } else {
            self.live.remove(&generation);
        }
        Ok(Some(self.handle(generation)))
    }
    pub(super) fn shutdown(&mut self) {
        self.closed = true;
        self.live.clear();
    }
}

impl RetainedHost {
    /// Host monotonic time. Windowless time advances only through `advance_time`.
    pub fn timer_now(&self) -> Instant {
        self.timers.now()
    }
    /// Schedules one delivery at or after `deadline`. Past deadlines are due on
    /// the next host dispatch. At most `MAX_HOST_TIMERS` timers may be live.
    pub fn timer_at(&mut self, deadline: Instant) -> Result<TimerHandle, TimerError> {
        self.timers.insert(deadline, None)
    }
    /// First expires after `interval`. Each dispatch delivers at most once, then
    /// rearms from that dispatch's time, discarding missed intervals after stalls.
    pub fn timer_every(&mut self, interval: Duration) -> Result<TimerHandle, TimerError> {
        if self.timers.closed {
            return Err(TimerError::Closed);
        }
        if interval.is_zero() {
            return Err(TimerError::ZeroInterval);
        }
        let deadline = self
            .timer_now()
            .checked_add(interval)
            .ok_or(TimerError::DeadlineOverflow)?;
        self.timers.insert(deadline, Some(interval))
    }
    /// Cancels immediately, including a due timer not yet delivered in this dispatch.
    /// Returns false for foreign, cancelled or already delivered one-shot handles.
    pub fn cancel_timer(&mut self, handle: &TimerHandle) -> bool {
        self.timers.cancel(handle)
    }
}
impl RetainedEventHost<'_> {
    pub fn timer_now(&self) -> Instant {
        self.host.timer_now()
    }
    /// Same bounded one-shot scheduling as `RetainedHost::timer_at`.
    pub fn timer_at(&mut self, deadline: Instant) -> Result<TimerHandle, TimerError> {
        self.host.timer_at(deadline)
    }
    /// Same non-bursting repeating schedule as `RetainedHost::timer_every`.
    pub fn timer_every(&mut self, interval: Duration) -> Result<TimerHandle, TimerError> {
        self.host.timer_every(interval)
    }
    pub fn cancel_timer(&mut self, handle: &TimerHandle) -> bool {
        self.host.cancel_timer(handle)
    }
}
