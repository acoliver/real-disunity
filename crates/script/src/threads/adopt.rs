//! Adopting threads that a restore left suspended, and the park kinds that say how.
//!
//! A thread restored from an original engine's save was suspended inside one of the engine's
//! wait natives: `Sleep`, `WaitWakeup` or `WaitMsg`. A restore continues the frames above that
//! native without calling it, so the scheduler never sees the thread yield. The host tells the
//! scheduler how the thread is parked, either with [`GameScripts::adopt`] or by letting the
//! game's own load code call the native bound with [`GameScripts::register_adopt_native`].

use mlua::{Function, Thread};

use super::natives;
use super::state::{Parking, id};
use crate::error::ScriptError;
use crate::original::GameScripts;

/// What an adopted thread is waiting for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdoptWait {
    /// A sleep with this many milliseconds left.
    Sleep(u64),
    /// A sleep until woken.
    Forever,
    /// A wait of the kind whose native is `kind`, with the milliseconds left before it times
    /// out, or none to wait until woken.
    Wait { kind: String, timeout: Option<u64> },
}

/// How a restored thread continues in the scheduler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adoption {
    /// The name of the clock the thread's remaining time counts on.
    pub clock: String,
    pub wait: AdoptWait,
    /// The game's flags on the thread. The engine does not read their bits.
    pub flags: i64,
    pub source: String,
}

/// A scheduler thread that carries flags, as a game saves it.
#[derive(Clone, Debug)]
pub struct PersistedThread {
    pub thread: Thread,
    pub flags: i64,
    pub source: String,
    pub clock: String,
    /// Milliseconds on its clock before it wakes by itself, `None` when it waits without a timeout.
    pub remaining: Option<u64>,
    /// The park kind of the native the thread waits in, if the game registered one.
    pub park: Option<String>,
}

impl GameScripts {
    /// Registers a restored thread as a scheduler thread parked as `adoption` says. The thread
    /// continues with the values of the wait: nothing for a sleep, and for a wait that times out
    /// the kind's timeout value, after its `on_resume` hook. Threads adopted earlier run first
    /// among those due together under the registration order.
    pub fn adopt(&self, thread: &Thread, adoption: Adoption) -> Result<(), ScriptError> {
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        let clock = scheduler
            .clock_index(&adoption.clock)
            .ok_or_else(|| refused(format!("no clock named {}", adoption.clock)))?;
        let parking = match adoption.wait {
            AdoptWait::Sleep(ms) => Parking::Sleep(Some(ms)),
            AdoptWait::Forever => Parking::Sleep(None),
            AdoptWait::Wait { kind, timeout } => {
                let wait = scheduler
                    .wait_index(&kind)
                    .ok_or_else(|| refused(format!("no wait named {kind}")))?;
                Parking::Wait(wait, timeout)
            }
        };
        scheduler
            .adopt(
                thread.clone(),
                clock,
                parking,
                (adoption.flags, adoption.source),
            )
            .map_err(refused)
    }

    /// Names the park kind of a native: a restored thread whose top frame waits in the native
    /// `native_path` is parked in `kind`. The scheduler's own sleep and wait natives are the
    /// ones whose kinds [`adopt_native`](Self::register_adopt_native) can adopt.
    pub fn register_park_native(&self, kind: &str, native_path: &str) -> Result<(), ScriptError> {
        if !self.natives().contains_key(native_path) {
            return Err(refused(format!("native {native_path} is not registered")));
        }
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        if scheduler
            .parks
            .get(native_path)
            .is_some_and(|other| other != kind)
        {
            return Err(refused(format!(
                "native {native_path} already has a park kind"
            )));
        }
        scheduler
            .parks
            .insert(native_path.to_owned(), kind.to_owned());
        Ok(())
    }

    /// Notes that a restored thread is parked in `kind`, as a restore does for a thread whose
    /// top frame waits in a native that has a park kind.
    pub fn note_parked(&self, thread: &Thread, kind: &str) {
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        scheduler.parked.insert(id(thread), kind.to_owned());
    }

    /// The kind a restored thread is parked in, until it is adopted.
    pub fn parked_kind(&self, thread: &Thread) -> Option<String> {
        let scheduler = self.scheduler().lock().expect("scheduler");
        scheduler.parked.get(&id(thread)).cloned()
    }

    /// The game's flags on a scheduler thread.
    pub fn thread_flags(&self, thread: &Thread) -> Option<i64> {
        self.scheduler().lock().expect("scheduler").flags_of(thread)
    }

    /// Sets the game's flags on a scheduler thread. Returns whether it is one.
    pub fn set_thread_flags(&self, thread: &Thread, flags: i64) -> bool {
        self.scheduler()
            .lock()
            .expect("scheduler")
            .set_flags(thread, flags)
    }

    /// The waiting threads whose flags share a bit with `mask`, in the order they were
    /// registered. A game saves these with their remaining times.
    pub fn persisted(&self, mask: i64) -> Vec<PersistedThread> {
        self.scheduler().lock().expect("scheduler").flagged(mask)
    }

    /// Registers a native `adopt(thread, clock, flags, source, remaining)` that a game's load
    /// code calls for each thread it persisted. `remaining` is the milliseconds left, nil for
    /// none. How the thread is parked comes from the kind noted when it was restored. It
    /// returns true, and raises an error for a thread that is not noted as parked.
    pub fn register_adopt_native(&mut self, path: &str) -> Result<(), ScriptError> {
        let function: Function = natives::adopt(self.lua(), self.scheduler())?;
        self.register_function(path, function)
    }
}

fn refused(message: String) -> ScriptError {
    ScriptError::Sandbox { message }
}
