//! Threads and timers for a game's original scripts.
//!
//! Original engines run game logic in coroutines that sleep on the game clock, under names such
//! as `CreateGameTimeThread` and `Sleep`. The engine supplies one scheduler and the game binds
//! its functions under the game's own names, as natives granted like any other.
//!
//! Time is whole milliseconds. [`GameScripts::advance`] moves the clock and resumes every thread
//! due by then, in order of wake time and then creation, each with the full instruction budget.
//! A thread sees the clock at its wake time, so one that sleeps again inside the advanced span runs
//! again within it. `Sleep(0)` yields to the next advance, so a loop of zero sleeps cannot spin
//! forever inside one. An error ends only the thread that raised it.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use mlua::{ChunkMode, Function, Lua, MultiValue, Table, Thread, ThreadStatus, Value};

use crate::error::ScriptError;
use crate::original::GameScripts;

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

type Key = (u64, u64);

struct Waiting {
    thread: Thread,
    args: MultiValue,
}

#[derive(Default)]
pub(crate) struct Scheduler {
    now: u64,
    next_seq: u64,
    queue: BTreeMap<Key, Waiting>,
    keys: HashMap<usize, Key>,
    /// New threads and threads that slept for zero, released at the start of the next advance.
    deferred: Vec<Waiting>,
    running: Option<Thread>,
    killed_running: bool,
}

impl Scheduler {
    fn schedule(&mut self, thread: Thread, wake: u64, args: MultiValue) {
        let key = (wake, self.next_seq);
        self.next_seq += 1;
        self.keys.insert(id(&thread), key);
        self.queue.insert(key, Waiting { thread, args });
    }

    fn remove(&mut self, thread: &Thread) -> bool {
        if let Some(key) = self.keys.remove(&id(thread)) {
            self.queue.remove(&key);
            return true;
        }
        let before = self.deferred.len();
        self.deferred
            .retain(|other| id(&other.thread) != id(thread));
        before != self.deferred.len()
    }

    /// The next thread due at or before `target`, with the clock moved to its wake time.
    fn next_due(&mut self, target: u64) -> Option<Waiting> {
        let (&key, _) = self.queue.first_key_value()?;
        if key.0 > target {
            return None;
        }
        let waiting = self.queue.remove(&key).expect("first key");
        self.keys.remove(&id(&waiting.thread));
        self.now = self.now.max(key.0);
        Some(waiting)
    }
}

fn id(thread: &Thread) -> usize {
    thread.to_pointer() as usize
}

pub(crate) type Shared = Arc<Mutex<Scheduler>>;

/// Yields the token and a whole number of milliseconds, or -1 to wait until woken.
const SLEEP: &str = r#"
local yield, token = ...
return function(ms)
    ms = ms or 0
    if type(ms) ~= "number" or ms ~= ms or ms < 0 then
        error("sleep time must be a non-negative number of milliseconds", 2)
    end
    if ms == math.huge then
        return yield(token, -1)
    end
    return yield(token, math.tointeger(math.ceil(ms)))
end
"#;

impl GameScripts {
    /// Registers the scheduler's functions as natives under the game's names.
    pub fn register_scheduler(&mut self, names: &SchedulerNames) -> Result<(), ScriptError> {
        let lua = self.lua().clone();
        let shared = Arc::clone(self.scheduler());
        let token = self.sleep_token().clone();
        let coroutine: Table = lua.globals().get("coroutine")?;
        let yield_fn: Function = coroutine.get("yield")?;
        let make: Function = lua
            .load(SLEEP)
            .set_name("=scheduler")
            .set_mode(ChunkMode::Text)
            .into_function()?;
        let sleep: Function = make.call((yield_fn, token))?;
        let natives = [
            (&names.spawn, spawn(&lua, &shared)?),
            (&names.sleep, sleep),
            (&names.wakeup, wakeup(&lua, &shared)?),
            (&names.kill, kill(&lua, &shared)?),
            (&names.current, current(&lua, &shared)?),
            (&names.now, now(&lua, &shared)?),
        ];
        for (name, function) in natives {
            self.register_function(name, function)?;
        }
        Ok(())
    }

    /// The scheduler clock, in milliseconds.
    pub fn now(&self) -> u64 {
        self.scheduler().lock().expect("scheduler").now
    }

    /// Threads waiting to run, including ones sleeping until woken.
    pub fn thread_count(&self) -> usize {
        let scheduler = self.scheduler().lock().expect("scheduler");
        scheduler.queue.len() + scheduler.deferred.len()
    }

    /// Advances the clock by `ms` and runs every thread due by then. Returns the threads that
    /// failed; the others keep running.
    pub fn advance(&self, ms: u64) -> Result<Vec<ThreadFailure>, ScriptError> {
        let target = {
            let mut scheduler = self.scheduler().lock().expect("scheduler");
            let now = scheduler.now;
            for waiting in std::mem::take(&mut scheduler.deferred) {
                scheduler.schedule(waiting.thread, now, waiting.args);
            }
            now.checked_add(ms)
                .ok_or_else(|| refused("the scheduler clock overflowed"))?
        };
        let mut failures = Vec::new();
        loop {
            let due = self.scheduler().lock().expect("scheduler").next_due(target);
            let Some(waiting) = due else { break };
            if let Some(failure) = self.resume(waiting)? {
                failures.push(failure);
            }
        }
        self.scheduler().lock().expect("scheduler").now = target;
        Ok(failures)
    }

    fn resume(&self, waiting: Waiting) -> Result<Option<ThreadFailure>, ScriptError> {
        let Waiting { thread, args } = waiting;
        {
            let mut scheduler = self.scheduler().lock().expect("scheduler");
            scheduler.running = Some(thread.clone());
            scheduler.killed_running = false;
        }
        self.instructions().store(0, Ordering::Relaxed);
        let result = self.budget().resume(self.lua(), &thread, args);
        let mut scheduler = self.scheduler().lock().expect("scheduler");
        scheduler.running = None;
        let yielded = match result {
            Err(error) => {
                return Ok(Some(ThreadFailure {
                    message: error.to_string(),
                }));
            }
            Ok(_) if thread.status() != ThreadStatus::Resumable => return Ok(None),
            Ok(values) => values,
        };
        if scheduler.killed_running {
            return Ok(None);
        }
        match sleep_request(&yielded, self.sleep_token()) {
            Some(0) | None => scheduler.deferred.push(Waiting {
                thread,
                args: MultiValue::new(),
            }),
            Some(ms) => {
                let wake =
                    u64::try_from(ms).map_or(u64::MAX, |ms| scheduler.now.saturating_add(ms));
                scheduler.schedule(thread, wake, MultiValue::new());
            }
        }
        Ok(None)
    }
}

/// The milliseconds a sleep yielded, -1 for until woken, or `None` for any other yield, which
/// waits for the next advance.
fn sleep_request(values: &MultiValue, token: &Table) -> Option<i64> {
    let mut values = values.iter();
    match (values.next(), values.next()) {
        (Some(Value::Table(first)), Some(Value::Integer(ms))) if first == token => Some(*ms),
        _ => None,
    }
}

fn spawn(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |lua, (function, args): (Function, MultiValue)| {
        let thread = lua.create_thread(function)?;
        // A new thread first runs at the next advance, even when spawned during one.
        shared.lock().expect("scheduler").deferred.push(Waiting {
            thread: thread.clone(),
            args,
        });
        Ok(thread)
    })
}

fn wakeup(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, thread: Thread| {
        let mut scheduler = shared.lock().expect("scheduler");
        let Some(&key) = scheduler.keys.get(&id(&thread)) else {
            return Ok(false);
        };
        let waiting = scheduler.queue.remove(&key).expect("indexed");
        scheduler.keys.remove(&id(&thread));
        let now = scheduler.now;
        scheduler.schedule(waiting.thread, now, waiting.args);
        Ok(true)
    })
}

fn kill(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, thread: Thread| {
        let mut scheduler = shared.lock().expect("scheduler");
        let running = scheduler
            .running
            .as_ref()
            .is_some_and(|running| id(running) == id(&thread));
        if running {
            scheduler.killed_running = true;
            return Ok(true);
        }
        Ok(scheduler.remove(&thread))
    })
}

fn current(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ()| Ok(shared.lock().expect("scheduler").running.clone()))
}

fn now(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ()| Ok(shared.lock().expect("scheduler").now))
}

fn refused(message: &str) -> ScriptError {
    ScriptError::Sandbox {
        message: message.into(),
    }
}
