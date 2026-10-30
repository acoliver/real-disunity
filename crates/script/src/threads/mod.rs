//! Threads and timers for a game's original scripts.
//!
//! Original engines run game logic in coroutines that sleep on the game clock, under names such
//! as `CreateGameTimeThread` and `Sleep`. The engine supplies one scheduler and the game binds
//! its functions under the game's own names, as natives granted like any other.
//!
//! Time is whole milliseconds. [`GameScripts::advance`] moves the clocks and resumes every thread
//! due by then, in order of wake time and then creation, each with the full instruction budget.
//! A thread sees its clock at its wake time, so one that sleeps again inside the advanced span
//! runs again within it. `Sleep(0)` yields to the next advance, so a loop of zero sleeps cannot
//! spin forever inside one. An error ends only the thread that raised it.
//!
//! A [`SchedulerPolicy`] selects the semantics of a game's original engine instead: truncated
//! fractions, negative sleeps that wait forever, zero sleeps and new threads that run within
//! the advance, run order by clock and registration, several clocks that can be paused and set,
//! and waits with timeouts that a wake or a timeout resumes with values.
//!
//! A thread restored from an original engine's save continues in the scheduler once the host
//! adopts it with the wait it was suspended in (see [`Adoption`]). Threads carry the game's
//! flags, and a save can keep only the threads whose flags match a mask.

mod adopt;
mod advance;
mod info;
mod lua;
mod natives;
mod policy;
mod register;
mod state;

pub use adopt::{AdoptWait, Adoption, PersistedThread};
pub use info::{ThreadInfo, ThreadWait};
pub(crate) use natives::Shared;
pub use policy::{
    ClockNative, FlagNatives, FlaggedSpawn, Fraction, NegativeSleep, NewThread, Order, ResumeValue,
    SchedulerPolicy, WaitKind, WakeNative, ZeroSleep,
};
pub(crate) use state::{Restored, ThreadRecord, Wake};

/// The names a game gives each scheduler function. Each becomes a native, granted per source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerNames {
    /// `spawn(f, ...)` creates a thread that calls `f(...)` at the next advance, and returns it.
    pub spawn: String,
    /// `sleep(ms)` suspends the calling thread; `math.huge` sleeps until woken.
    pub sleep: String,
    /// `wakeup(thread)` makes a sleeping thread due now; returns whether it was waiting.
    pub wakeup: String,
    /// `kill(thread)` removes a thread; returns whether it existed.
    pub kill: String,
    /// `current()` returns the running scheduler thread, or nil outside one.
    pub current: String,
    /// `now()` returns the scheduler clock in milliseconds.
    pub now: String,
}

/// A thread that raised an error during an advance, and has ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadFailure {
    pub message: String,
}
