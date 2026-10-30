//! The scheduler's registry: clocks with their queues of waiting threads.

use std::collections::{BTreeMap, HashMap};

use mlua::{Function, MultiValue, Thread};

use super::PersistedThread;
use super::policy::{NewThread, Order, ResumeValue, SchedulerPolicy, ZeroSleep};
use crate::persist::ThreadSchedule;

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
    /// The game's flags on the thread; the engine does not read their bits.
    pub flags: i64,
    pub source: String,
}

impl Origin {
    pub(crate) fn new(start: Option<(Function, MultiValue)>) -> Self {
        Self {
            start,
            flags: 0,
            source: String::new(),
        }
    }
}

/// How an adopted thread is parked: the wait it was suspended in when it was saved.
pub(crate) enum Parking {
    /// `Some(ms)` more to sleep, or `None` until woken.
    Sleep(Option<u64>),
    /// In the wait kind at this index, with the milliseconds left before it times out.
    Wait(usize, Option<u64>),
}

/// A thread restored from a save, ready to be scheduled.
pub(crate) struct Restored {
    pub thread: Thread,
    /// Set for a thread that had not run; it starts from the beginning.
    pub start: Option<(Function, MultiValue)>,
    pub wake: Wake,
    pub schedule: ThreadSchedule,
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
    pub schedule: ThreadSchedule,
}

pub(crate) struct ClockState {
    pub name: String,
    pub now: u64,
    pub paused: bool,
    queue: BTreeMap<Key, Waiting>,
}

/// A wait kind as the scheduler runs it.
pub(crate) struct WaitSpec {
    /// The path of the native that waits.
    pub native: String,
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
    /// The path of the native that sleeps.
    pub(crate) sleep_native: String,
    /// The park kind the game gave each native, by path.
    pub(crate) parks: BTreeMap<String, String>,
    /// Restored threads noted as parked, by the kind, until they are adopted.
    pub(crate) parked: HashMap<usize, String>,
    keys: HashMap<usize, (usize, Key)>,
    /// Threads released at the start of the next advance.
    deferred: Vec<Waiting>,
    running: Option<Thread>,
    running_clock: usize,
    pub(crate) killed_running: bool,
    /// Set by the game's interrupt native; ends the advance in progress.
    pub(crate) interrupted: bool,
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
            sleep_native: String::new(),
            parks: BTreeMap::new(),
            parked: HashMap::new(),
            keys: HashMap::new(),
            deferred: Vec::new(),
            running: None,
            running_clock: 0,
            killed_running: false,
            interrupted: false,
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

    /// Whether every waiting thread sleeps on the default clock, which a save of all threads
    /// can describe without the game saying which of its threads it persists.
    pub(crate) fn is_plain(&self) -> bool {
        let plain = |waiting: &Waiting| waiting.clock == 0 && waiting.wait.is_none();
        self.deferred.iter().all(plain)
            && self
                .clocks
                .iter()
                .all(|clock| clock.queue.values().all(plain))
    }

    /// Whether a save can describe every waiting thread. A wait that was woken but has not
    /// run yet holds the values it will resume with, which a save cannot keep.
    pub(crate) fn is_saveable(&self) -> bool {
        let plain = |waiting: &Waiting| waiting.wait.is_none() || !waiting.woken;
        self.deferred.iter().all(plain)
            && self
                .clocks
                .iter()
                .all(|clock| clock.queue.values().all(plain))
    }

    /// Every waiting thread, in the order it would run: by clock, then wake time, then those
    /// waiting for the next advance.
    pub(crate) fn records(&self) -> Vec<ThreadRecord> {
        let record = |waiting: &Waiting, wake| {
            let origin = &self.origins[&id(&waiting.thread)];
            let native = match waiting.wait {
                Some(kind) => &self.waits[kind].native,
                None => &self.sleep_native,
            };
            ThreadRecord {
                thread: waiting.thread.clone(),
                start: origin.start.clone(),
                wake,
                schedule: ThreadSchedule {
                    clock: Some(self.clocks[waiting.clock].name.clone()),
                    wait: waiting.wait.map(|kind| self.waits[kind].native.clone()),
                    flags: origin.flags,
                    source: origin.source.clone(),
                    park: self.parks.get(native).cloned(),
                },
            }
        };
        let timed = self.clocks.iter().flat_map(|clock| {
            clock.queue.iter().map(|(&(at, _), waiting)| {
                let wake = if at == u64::MAX {
                    Wake::UntilWoken
                } else {
                    Wake::At(at)
                };
                record(waiting, wake)
            })
        });
        let next = self
            .deferred
            .iter()
            .map(|waiting| record(waiting, Wake::NextAdvance));
        timed.chain(next).collect()
    }

    /// Schedules restored threads, with each clock of `times` set to its saved time.
    pub(crate) fn restore(
        &mut self,
        times: &[(String, u64)],
        threads: Vec<Restored>,
    ) -> Result<(), String> {
        for (name, now) in times {
            let index = self
                .clock_index(name)
                .ok_or(format!("no clock named {name}"))?;
            self.clocks[index].now = *now;
        }
        for restored in threads {
            let (clock, wait) = self.resolve(&restored.schedule)?;
            let Restored {
                thread,
                start,
                wake,
                schedule,
            } = restored;
            let args = start
                .as_ref()
                .map_or_else(MultiValue::new, |(_, a)| a.clone());
            let mut origin = Origin::new(start);
            origin.flags = schedule.flags;
            origin.source = schedule.source;
            self.origins.insert(id(&thread), origin);
            if let Some(kind) = schedule.park {
                self.parked.insert(id(&thread), kind);
            }
            let mut waiting = self.waiting(thread, args, clock);
            waiting.wait = wait;
            match wake {
                Wake::NextAdvance => self.deferred.push(waiting),
                Wake::At(at) => self.schedule(waiting, at),
                Wake::UntilWoken => self.schedule(waiting, u64::MAX),
            }
        }
        Ok(())
    }

    /// The clock and wait kind a saved thread names.
    fn resolve(&self, schedule: &ThreadSchedule) -> Result<(usize, Option<usize>), String> {
        let clock = match &schedule.clock {
            None => 0,
            Some(name) => self
                .clock_index(name)
                .ok_or(format!("no clock named {name}"))?,
        };
        let wait = match &schedule.wait {
            None => None,
            Some(native) => Some(
                self.wait_index(native)
                    .ok_or(format!("no wait named {native}"))?,
            ),
        };
        Ok((clock, wait))
    }

    pub(crate) fn wait_index(&self, native: &str) -> Option<usize> {
        self.waits.iter().position(|wait| wait.native == native)
    }

    /// Registers a thread that a restore left suspended in a wait, as parked how `parking`
    /// says, on clock `clock`. The thread is filed after those adopted before it.
    pub(crate) fn adopt(
        &mut self,
        thread: Thread,
        clock: usize,
        parking: Parking,
        game: (i64, String),
    ) -> Result<(), String> {
        if self.is_running_thread(&thread) {
            return Err("a running thread cannot be adopted".into());
        }
        self.remove(&thread);
        self.parked.remove(&id(&thread));
        let mut origin = Origin::new(None);
        (origin.flags, origin.source) = game;
        self.origins.insert(id(&thread), origin);
        let mut waiting = self.waiting(thread, MultiValue::new(), clock);
        let (wait, timeout) = match parking {
            Parking::Sleep(timeout) => (None, timeout),
            Parking::Wait(kind, timeout) => (Some(kind), timeout),
        };
        waiting.wait = wait;
        let now = self.clocks[clock].now;
        self.schedule(
            waiting,
            timeout.map_or(u64::MAX, |ms| now.saturating_add(ms)),
        );
        Ok(())
    }

    /// How a thread noted as parked in `kind` is parked, given the time left.
    pub(crate) fn parking_of(&self, kind: &str, remaining: Option<u64>) -> Result<Parking, String> {
        let native = self
            .parks
            .iter()
            .find_map(|(native, other)| (other == kind).then_some(native))
            .ok_or(format!("park kind {kind} has no native"))?;
        if *native == self.sleep_native {
            return Ok(Parking::Sleep(remaining));
        }
        let wait = self
            .wait_index(native)
            .ok_or(format!("park kind {kind} is not a scheduler wait"))?;
        Ok(Parking::Wait(wait, remaining))
    }

    /// Live threads whose flags share a bit with `mask`, in registration order.
    pub(crate) fn flagged(&self, mask: i64) -> Vec<PersistedThread> {
        let mut found: Vec<(u64, PersistedThread)> = Vec::new();
        let mut add = |waiting: &Waiting, wake: Option<u64>| {
            let origin = &self.origins[&id(&waiting.thread)];
            if origin.flags & mask == 0 {
                return;
            }
            let clock = &self.clocks[waiting.clock];
            let native = match waiting.wait {
                Some(kind) => &self.waits[kind].native,
                None => &self.sleep_native,
            };
            found.push((
                waiting.reg,
                PersistedThread {
                    thread: waiting.thread.clone(),
                    flags: origin.flags,
                    source: origin.source.clone(),
                    clock: clock.name.clone(),
                    remaining: wake.map(|at| at.saturating_sub(clock.now)),
                    park: self.parks.get(native).cloned(),
                },
            ));
        };
        for clock in &self.clocks {
            for (&(at, _), waiting) in &clock.queue {
                add(waiting, (at != u64::MAX).then_some(at));
            }
        }
        for waiting in &self.deferred {
            add(waiting, Some(self.clocks[waiting.clock].now));
        }
        found.sort_by_key(|entry| entry.0);
        found.into_iter().map(|(_, persisted)| persisted).collect()
    }

    pub(crate) fn set_flags(&mut self, thread: &Thread, flags: i64) -> bool {
        self.origins
            .get_mut(&id(thread))
            .map(|o| o.flags = flags)
            .is_some()
    }

    /// Adds the bits of `flags` to a scheduler thread's flags. Returns whether it is one.
    pub(crate) fn add_flags(&mut self, thread: &Thread, flags: i64) -> bool {
        self.origins
            .get_mut(&id(thread))
            .map(|o| o.flags |= flags)
            .is_some()
    }

    pub(crate) fn set_source(&mut self, thread: &Thread, source: &str) -> bool {
        self.origins
            .get_mut(&id(thread))
            .map(|o| source.clone_into(&mut o.source))
            .is_some()
    }

    /// The name of the clock a scheduler thread belongs to.
    pub(crate) fn clock_name_of(&self, thread: &Thread) -> Option<String> {
        let index = if self.is_running_thread(thread) {
            self.running_clock
        } else if let Some(&(clock, _)) = self.keys.get(&id(thread)) {
            clock
        } else {
            self.deferred
                .iter()
                .find(|waiting| id(&waiting.thread) == id(thread))?
                .clock
        };
        Some(self.clocks[index].name.clone())
    }

    pub(crate) fn flags_of(&self, thread: &Thread) -> Option<i64> {
        self.origins.get(&id(thread)).map(|origin| origin.flags)
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
        self.interrupted = false;
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
