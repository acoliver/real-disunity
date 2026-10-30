//! How a game's original scheduler differs from the engine's defaults.
//!
//! The default policy is the engine's own behaviour. A game that needs the semantics of its
//! original engine selects them with [`GameScripts::register_scheduler_with`].
//!
//! [`GameScripts::register_scheduler_with`]: crate::GameScripts::register_scheduler_with

use mlua::{Lua, Value};

/// How a fractional number of milliseconds becomes a whole one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fraction {
    /// Rounds up, so a thread never wakes before the time it asked for.
    #[default]
    Ceil,
    /// Cuts the fraction off, as original engines do with `int()`: 19.53 ms is 19 ms.
    Truncate,
}

/// What a sleep of a negative number of milliseconds does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NegativeSleep {
    /// The sleep raises a Lua error in the calling thread.
    #[default]
    Error,
    /// The thread waits without a timeout until it is woken.
    Forever,
}

/// When a thread that slept for zero milliseconds runs again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZeroSleep {
    /// At the next advance, so a loop of zero sleeps runs once per advance.
    #[default]
    NextAdvance,
    /// It is due at once and may run again within the same advance. A loop of zero sleeps is
    /// ended by [`SchedulerPolicy::max_zero_sleeps`].
    SameAdvance,
}

/// When a new thread first runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewThread {
    /// At the next advance, even when it was created during one.
    #[default]
    NextAdvance,
    /// It is due when created, so one created during an advance runs in that advance.
    SameAdvance,
}

/// The order in which threads due at the same time run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// By wake time, then by when the thread last went to sleep.
    #[default]
    ByWakeThenReschedule,
    /// By wake time, then by clock in the order the policy lists them, then by the order the
    /// threads were created, which sleeping again does not change.
    ByWakeThenClockThenRegistration,
}

/// A value a wait resumes its thread with when it times out.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ResumeValue {
    #[default]
    Nil,
    Boolean(bool),
    Integer(i64),
    Text(String),
}

impl ResumeValue {
    pub(crate) fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        Ok(match self {
            Self::Nil => Value::Nil,
            Self::Boolean(value) => Value::Boolean(*value),
            Self::Integer(value) => Value::Integer(*value),
            Self::Text(text) => Value::String(lua.create_string(text)?),
        })
    }
}

/// A native bound under a game's name for one clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockNative {
    pub native: String,
    /// The name of a clock in [`SchedulerPolicy::clocks`].
    pub clock: String,
}

/// A wait other than sleep, such as `WaitWakeup` or `WaitMsg`.
#[derive(Clone, Debug, PartialEq)]
pub struct WaitKind {
    /// `wait(timeout)` suspends the calling thread. A nil, negative or non-numeric timeout
    /// waits without one. It returns the values the thread is resumed with.
    pub native: String,
    /// What a wait that times out returns.
    pub timeout_value: ResumeValue,
    /// The path of a registered native called with the thread just before it resumes, whether it
    /// timed out or was woken, so the game can unlink it from its own tables.
    pub on_resume: Option<String>,
}

/// The behaviour of a game's scheduler. [`Default`] is the engine's own.
#[derive(Clone, Debug, PartialEq)]
pub struct SchedulerPolicy {
    pub fraction: Fraction,
    pub negative_sleep: NegativeSleep,
    pub zero_sleep: ZeroSleep,
    pub new_thread: NewThread,
    pub order: Order,
    /// How many times in a row a thread may sleep for zero within one advance before it is
    /// ended with a failure, under [`ZeroSleep::SameAdvance`].
    pub max_zero_sleeps: u32,
    /// The clocks, each with its own time. The first is the default: the one `now` reads
    /// outside a thread and the one `spawn` creates threads on.
    pub clocks: Vec<String>,
    /// Natives that create a thread on a named clock, like `spawn`.
    pub spawn_on: Vec<ClockNative>,
    /// Natives that return the time of a named clock.
    pub read_clock: Vec<ClockNative>,
    /// The waits besides sleep.
    pub waits: Vec<WaitKind>,
    /// `wake(thread, ...)` makes a thread in a wait due now and resumes it with the values.
    /// It returns whether the thread was in a wait.
    pub wake: Option<String>,
}

impl Default for SchedulerPolicy {
    fn default() -> Self {
        Self {
            fraction: Fraction::default(),
            negative_sleep: NegativeSleep::default(),
            zero_sleep: ZeroSleep::default(),
            new_thread: NewThread::default(),
            order: Order::default(),
            max_zero_sleeps: 100_000,
            clocks: vec!["game".into()],
            spawn_on: Vec::new(),
            read_clock: Vec::new(),
            waits: Vec::new(),
            wake: None,
        }
    }
}
