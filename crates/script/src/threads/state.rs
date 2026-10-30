//! The scheduler's registry: clocks with their queues of waiting threads.

use std::collections::{BTreeMap, HashMap};

use mlua::{Function, MultiValue, Thread};

use super::policy::{NewThread, Order, ResumeValue, SchedulerPolicy, ZeroSleep};

/// Wake time on a clock, then the tie-break the policy's order picks.
type Key = (u64, u64);

pub(crate) fn id(thread: &Thread) -> usize {
    thread.to_pointer() as usize
}

/// A thread waiting to run, and what it needs to be filed again.
pub(crate) struct Waiting {
    pub thread: Thread,
    pub args: MultiValue,
    /// The index of the clock the thread belongs to.
    pub clock: usize,
    /// Fixed when the thread is created; the tie-break under registration order.
    pub reg: u64,
    /// The wait kind it is suspended in, if it is not a sleeper.
    pub wait: Option<usize>,
    /// Set when the wait ended by a wake rather than its timeout.
    pub woken: bool,
    /// Zero sleeps in a row.
    pub zeros: u32,
}

/// What a thread has not run yet: the function and arguments it was spawned with. A thread
/// that has run is saved by capturing where it stopped, so this is cleared on its first resume.
pub(crate) struct Origin {
    pub start: Option<(Function, MultiValue)>,
}

/// A thread restored from a save, ready to be scheduled.
pub(crate) struct Restored {
    pub thread: Thread,
    /// Set for a thread that had not run; it starts from the beginning.
    pub start: Option<(Function, MultiValue)>,
    pub wake: Wake,
}

/// When a saved thread runs next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wake {
    NextAdvance,
    At(u64),
    UntilWoken,
}

/// A live scheduler thread as saving sees it.
pub(crate) struct ThreadRecord {
    pub thread: Thread,
    /// Set for a thread that has not run yet.
    pub start: Option<(Function, MultiValue)>,
    pub wake: Wake,
}

pub(crate) struct ClockState {
    pub name: String,
    pub now: u64,
    pub paused: bool,
    queue: BTreeMap<Key, Waiting>,
}

/// A wait kind as the scheduler runs it.
pub(crate) struct WaitSpec {
    pub timeout: ResumeValue,
    pub on_resume: Option<Function>,
}

pub(crate) struct Rules {
    order: Order,
    zero_sleep: ZeroSleep,
    new_thread: NewThread,
    max_zeros: u32,
}

pub(crate) struct Scheduler {
    pub(crate) clocks: Vec<ClockState>,
    pub(crate) waits: Vec<WaitSpec>,
    rules: Rules,
    next_seq: u64,
    next_reg: u64,
    keys: HashMap<usize, (usize, Key)>,
    /// Threads released at the start of the next advance.
    deferred: Vec<Waiting>,
    running: Option<Thread>,
    running_clock: usize,
    pub(crate) killed_running: bool,
    pub(crate) origins: HashMap<usize, Origin>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(&SchedulerPolicy::default(), Vec::new())
    }
}

impl Scheduler {
    pub(crate) fn new(policy: &SchedulerPolicy, waits: Vec<WaitSpec>) -> Self {
        let clocks = policy.clocks.iter().map(|name| ClockState {
            name: name.clone(),
            now: 0,
            paused: false,
            queue: BTreeMap::new(),
        });
        Self {
            clocks: clocks.collect(),
            waits,
            rules: Rules {
                order: policy.order,
                zero_sleep: policy.zero_sleep,
                new_thread: policy.new_thread,
                max_zeros: policy.max_zero_sleeps,
            },
            next_seq: 0,
            next_reg: 0,
            keys: HashMap::new(),
            deferred: Vec::new(),
            running: None,
            running_clock: 0,
            killed_running: false,
            origins: HashMap::new(),
        }
    }

    pub(crate) fn clock_index(&self, name: &str) -> Option<usize> {
        self.clocks.iter().position(|clock| clock.name == name)
    }

    /// The time of the default clock.
    pub(crate) fn now(&self) -> u64 {
        self.clocks[0].now
    }

    /// The time of the clock of the running thread, or of the default clock outside one.
    pub(crate) fn running_now(&self) -> u64 {
        let clock = if self.running.is_some() {
            self.running_clock
        } else {
            0
        };
        self.clocks[clock].now
    }

    pub(crate) fn queued(&self) -> usize {
        self.clocks
            .iter()
            .map(|clock| clock.queue.len())
            .sum::<usize>()
            + self.deferred.len()
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.queued() == 0 && self.running.is_none()
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(crate) fn running(&self) -> Option<Thread> {
        self.running.clone()
    }

    pub(crate) fn is_running_thread(&self, thread: &Thread) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| id(running) == id(thread))
    }

    pub(crate) fn begin_run(&mut self, waiting: &Waiting) {
        self.running = Some(waiting.thread.clone());
        self.running_clock = waiting.clock;
        self.killed_running = false;
        if let Some(origin) = self.origins.get_mut(&id(&waiting.thread)) {
            origin.start = None;
        }
    }

    pub(crate) fn end_run(&mut self) {
        self.running = None;
    }

    /// Whether a save can describe every waiting thread: they are on the default clock and
    /// sleeping. Threads on other clocks and in waits are saved with adoption (#147).
    pub(crate) fn is_saveable(&self) -> bool {
        let plain = |waiting: &Waiting| waiting.clock == 0 && waiting.wait.is_none();
        self.deferred.iter().all(plain)
            && self
                .clocks
                .iter()
                .all(|clock| clock.queue.values().all(plain))
    }

    /// Every waiting thread, in the order it would run: by wake time, then those waiting for
    /// the next advance.
    pub(crate) fn records(&self) -> Vec<ThreadRecord> {
        let record = |thread: &Thread, wake| ThreadRecord {
            thread: thread.clone(),
            start: self.origins[&id(thread)].start.clone(),
            wake,
        };
        let timed = self.clocks[0].queue.iter().map(|(&(at, _), waiting)| {
            let wake = if at == u64::MAX {
                Wake::UntilWoken
            } else {
                Wake::At(at)
            };
            record(&waiting.thread, wake)
        });
        let next = self
            .deferred
            .iter()
            .map(|waiting| record(&waiting.thread, Wake::NextAdvance));
        timed.chain(next).collect()
    }

    /// Schedules restored threads on the default clock with its time at `now`.
    pub(crate) fn restore(&mut self, now: u64, threads: Vec<Restored>) {
        self.clocks[0].now = now;
        for Restored {
            thread,
            start,
            wake,
        } in threads
        {
            let args = start
                .as_ref()
                .map_or_else(MultiValue::new, |(_, a)| a.clone());
            self.origins.insert(id(&thread), Origin { start });
            let waiting = self.waiting(thread, args, 0);
            match wake {
                Wake::NextAdvance => self.deferred.push(waiting),
                Wake::At(at) => self.schedule(waiting, at),
                Wake::UntilWoken => self.schedule(waiting, u64::MAX),
            }
        }
    }

    pub(crate) fn waiting(&mut self, thread: Thread, args: MultiValue, clock: usize) -> Waiting {
        let reg = self.next_reg;
        self.next_reg += 1;
        Waiting {
            thread,
            args,
            clock,
            reg,
            wait: None,
            woken: false,
            zeros: 0,
        }
    }

    /// Adds a new thread, to run at the next advance or, under the policy, when it is due.
    pub(crate) fn add_new(&mut self, waiting: Waiting) {
        match self.rules.new_thread {
            NewThread::NextAdvance => self.deferred.push(waiting),
            NewThread::SameAdvance => {
                let now = self.clocks[waiting.clock].now;
                self.schedule(waiting, now);
            }
        }
    }

    pub(crate) fn schedule(&mut self, waiting: Waiting, wake: u64) {
        let tie = match self.rules.order {
            Order::ByWakeThenReschedule => {
                self.next_seq += 1;
                self.next_seq
            }
            Order::ByWakeThenClockThenRegistration => waiting.reg,
        };
        let key = (wake, tie);
        self.keys.insert(id(&waiting.thread), (waiting.clock, key));
        self.clocks[waiting.clock].queue.insert(key, waiting);
    }

    /// Files a thread that slept for `ms` milliseconds, `None` for until woken. Returns the
    /// message of the failure that ends the thread when it sleeps for zero too often.
    pub(crate) fn file_sleep(&mut self, mut waiting: Waiting, ms: Option<u64>) -> Option<String> {
        let now = self.clocks[waiting.clock].now;
        if ms != Some(0) {
            waiting.zeros = 0;
        } else if waiting.wait.is_none() && self.rules.zero_sleep == ZeroSleep::NextAdvance {
            self.deferred.push(waiting);
            return None;
        } else {
            waiting.zeros += 1;
            if waiting.zeros > self.rules.max_zeros {
                self.origins.remove(&id(&waiting.thread));
                return Some(format!(
                    "thread slept for zero more than {} times in one advance",
                    self.rules.max_zeros
                ));
            }
        }
        let wake = ms.map_or(u64::MAX, |ms| now.saturating_add(ms));
        self.schedule(waiting, wake);
        None
    }

    /// Files a thread that yielded something other than a sleep, to run at the next advance.
    pub(crate) fn file_deferred(&mut self, mut waiting: Waiting) {
        waiting.zeros = 0;
        waiting.wait = None;
        self.deferred.push(waiting);
    }

    pub(crate) fn remove(&mut self, thread: &Thread) -> bool {
        self.origins.remove(&id(thread));
        if self.take(thread).is_some() {
            return true;
        }
        let before = self.deferred.len();
        self.deferred
            .retain(|other| id(&other.thread) != id(thread));
        before != self.deferred.len()
    }

    /// Takes a timed thread out of its queue.
    fn take(&mut self, thread: &Thread) -> Option<Waiting> {
        let (clock, key) = self.keys.remove(&id(thread))?;
        self.clocks[clock].queue.remove(&key)
    }

    /// Makes a timed thread due now. With `values` it must be in a wait and is resumed with them.
    pub(crate) fn wake(&mut self, thread: &Thread, values: Option<MultiValue>) -> bool {
        let Some(&(clock, key)) = self.keys.get(&id(thread)) else {
            return false;
        };
        if values.is_some() && self.clocks[clock].queue[&key].wait.is_none() {
            return false;
        }
        let mut waiting = self.take(thread).expect("indexed");
        waiting.woken = true;
        if let Some(values) = values {
            waiting.args = values;
        }
        let now = self.clocks[clock].now;
        self.schedule(waiting, now);
        true
    }

    /// Moves threads waiting for the next advance onto their clocks, due now. Returns the time
    /// of each clock, which the advance measures from.
    pub(crate) fn begin_advance(&mut self, ms: u64) -> Option<Vec<u64>> {
        for waiting in std::mem::take(&mut self.deferred) {
            let now = self.clocks[waiting.clock].now;
            self.schedule(waiting, now);
        }
        for clock in &self.clocks {
            if !clock.paused {
                clock.now.checked_add(ms)?;
            }
        }
        Some(self.clocks.iter().map(|clock| clock.now).collect())
    }

    /// The next thread due within `ms` of `start`, and its time as milliseconds into the
    /// advance. The clocks that run move to it.
    pub(crate) fn next_due(
        &mut self,
        start: &[u64],
        ms: u64,
        elapsed: u64,
    ) -> Option<(Waiting, u64)> {
        let mut best: Option<(u64, usize)> = None;
        for (index, clock) in self.clocks.iter().enumerate() {
            let Some((&(wake, _), _)) = clock.queue.first_key_value() else {
                continue;
            };
            let at = wake.saturating_sub(start[index]).max(elapsed);
            if clock.paused || at > ms || best.is_some_and(|(other, _)| other <= at) {
                continue;
            }
            best = Some((at, index));
        }
        let (at, index) = best?;
        let (_, waiting) = self.clocks[index].queue.pop_first().expect("first key");
        self.keys.remove(&id(&waiting.thread));
        self.set_elapsed(start, at);
        Some((waiting, at))
    }

    /// Moves every clock that is not paused to `elapsed` ms after `start`.
    pub(crate) fn set_elapsed(&mut self, start: &[u64], elapsed: u64) {
        for (clock, &from) in self.clocks.iter_mut().zip(start) {
            if !clock.paused {
                clock.now = clock.now.max(from.saturating_add(elapsed));
            }
        }
    }
}
