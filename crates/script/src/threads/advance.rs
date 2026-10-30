//! Advancing the clocks and running the threads that come due.

use std::sync::atomic::Ordering;

use mlua::{MultiValue, Table, Thread, ThreadStatus, Value};

use super::ThreadFailure;
use super::state::{Waiting, id};
use crate::error::ScriptError;
use crate::original::GameScripts;

/// What a thread's yield asks for.
enum Request {
    /// Any yield that is not the scheduler's: run again at the next advance.
    Other,
    /// Sleep, or wait of the given kind, for a whole number of ms; negative for until woken.
    Wait(i64, Option<usize>),
}

impl GameScripts {
    /// The time of the default clock, in milliseconds.
    pub fn now(&self) -> u64 {
        self.scheduler().lock().expect("scheduler").now()
    }

    /// The time of the clock called `name`, if the scheduler has one.
    pub fn clock_now(&self, name: &str) -> Option<u64> {
        let scheduler = self.scheduler().lock().expect("scheduler");
        Some(scheduler.clocks[scheduler.clock_index(name)?].now)
    }

    /// Sets the time of the clock called `name`. Threads already waiting keep their wake times.
    pub fn set_clock(&self, name: &str, ms: u64) -> Result<(), ScriptError> {
        self.with_clock(name, |clock| clock.now = ms)
    }

    /// A paused clock does not move in an advance, and its threads do not run.
    pub fn set_clock_paused(&self, name: &str, paused: bool) -> Result<(), ScriptError> {
        self.with_clock(name, |clock| clock.paused = paused)
    }

    fn with_clock(
        &self,
        name: &str,
        change: impl FnOnce(&mut super::state::ClockState),
    ) -> Result<(), ScriptError> {
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        let index = scheduler
            .clock_index(name)
            .ok_or_else(|| ScriptError::Sandbox {
                message: format!("no clock named {name}"),
            })?;
        change(&mut scheduler.clocks[index]);
        Ok(())
    }

    /// Threads waiting to run, including ones sleeping until woken.
    pub fn thread_count(&self) -> usize {
        self.scheduler().lock().expect("scheduler").queued()
    }

    /// Makes a thread in a wait due now and resumes it with `values`, as a native bound to the
    /// policy's `wake` does. Returns whether the thread was in a wait.
    pub fn wake_thread(&self, thread: &Thread, values: Vec<Value>) -> bool {
        let values = MultiValue::from_iter(values);
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        scheduler.wake(thread, Some(values))
    }

    /// Advances the clocks by `ms` and runs every thread due by then. Returns the threads that
    /// failed; the others keep running.
    pub fn advance(&self, ms: u64) -> Result<Vec<ThreadFailure>, ScriptError> {
        let start = {
            let mut scheduler = self.scheduler().lock().expect("scheduler");
            scheduler
                .begin_advance(ms)
                .ok_or_else(|| refused("the scheduler clock overflowed"))?
        };
        let mut failures = Vec::new();
        let mut elapsed = 0;
        loop {
            let due = self
                .scheduler()
                .lock()
                .expect("scheduler")
                .next_due(&start, ms, elapsed);
            let Some((waiting, at)) = due else { break };
            elapsed = at;
            if let Some(failure) = self.resume(waiting)? {
                failures.push(failure);
            }
        }
        self.scheduler()
            .lock()
            .expect("scheduler")
            .set_elapsed(&start, ms);
        Ok(failures)
    }

    /// What a waiting thread is resumed with: its arguments, or for a wait that timed out the
    /// kind's timeout value. The kind's `on_resume` hook runs first.
    fn delivery(&self, waiting: &mut Waiting) -> mlua::Result<MultiValue> {
        let Some(kind) = waiting.wait else {
            return Ok(std::mem::take(&mut waiting.args));
        };
        let (timeout, hook) = {
            let scheduler = self.scheduler().lock().expect("scheduler");
            let spec = &scheduler.waits[kind];
            (spec.timeout.clone(), spec.on_resume.clone())
        };
        if let Some(hook) = hook {
            hook.call::<()>(waiting.thread.clone())?;
        }
        if waiting.woken {
            return Ok(std::mem::take(&mut waiting.args));
        }
        Ok(MultiValue::from_iter([timeout.to_lua(self.lua())?]))
    }

    fn resume(&self, mut waiting: Waiting) -> Result<Option<ThreadFailure>, ScriptError> {
        let thread = waiting.thread.clone();
        let args = self.delivery(&mut waiting);
        self.scheduler()
            .lock()
            .expect("scheduler")
            .begin_run(&waiting);
        self.instructions().store(0, Ordering::Relaxed);
        let (result, overrun) = match args {
            Ok(args) => self.budget().resume_scheduled(self.lua(), &thread, args),
            Err(error) => (Err(error), None),
        };
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        scheduler.end_run();
        let killed = scheduler.killed_running;
        let ended = result.is_err()
            || overrun.is_some()
            || thread.status() != ThreadStatus::Resumable
            || killed;
        if ended {
            scheduler.origins.remove(&id(&thread));
        }
        if let Some(message) = overrun {
            return Ok(Some(ThreadFailure { message }));
        }
        let yielded = match result {
            Err(error) => {
                return Ok(Some(ThreadFailure {
                    message: error.to_string(),
                }));
            }
            Ok(_) if thread.status() != ThreadStatus::Resumable || killed => return Ok(None),
            Ok(values) => values,
        };
        waiting.woken = false;
        let message = match request(&yielded, self.sleep_token(), scheduler.waits.len()) {
            Request::Other => {
                scheduler.file_deferred(waiting);
                None
            }
            Request::Wait(ms, kind) => {
                waiting.wait = kind;
                scheduler.file_sleep(waiting, u64::try_from(ms).ok())
            }
        };
        Ok(message.map(|message| ThreadFailure { message }))
    }
}

/// Reads a yield: the token, a number of milliseconds, and for a wait the kind.
fn request(values: &MultiValue, token: &Table, kinds: usize) -> Request {
    let mut values = values.iter();
    let (Some(Value::Table(first)), Some(Value::Integer(ms))) = (values.next(), values.next())
    else {
        return Request::Other;
    };
    if first != token {
        return Request::Other;
    }
    match values.next() {
        None => Request::Wait(*ms, None),
        Some(Value::Integer(kind)) => match usize::try_from(*kind) {
            Ok(kind) if kind < kinds => Request::Wait(*ms, Some(kind)),
            _ => Request::Other,
        },
        Some(_) => Request::Other,
    }
}

fn refused(message: &str) -> ScriptError {
    ScriptError::Sandbox {
        message: message.into(),
    }
}
