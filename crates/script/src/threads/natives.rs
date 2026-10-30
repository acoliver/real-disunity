//! The Rust natives of the scheduler.

use std::sync::{Arc, Mutex};

use mlua::{Function, Lua, MultiValue, Thread};

use super::state::{Origin, Scheduler, id};

pub(crate) type Shared = Arc<Mutex<Scheduler>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Scheduler> {
    shared.lock().expect("scheduler")
}

/// `spawn(f, ...)` on the clock at index `clock`.
pub(crate) fn spawn(lua: &Lua, shared: &Shared, clock: usize) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |lua, (function, args): (Function, MultiValue)| {
        let thread = lua.create_thread(function.clone())?;
        let mut scheduler = lock(&shared);
        scheduler
            .origins
            .insert(id(&thread), Origin::new(Some((function, args.clone()))));
        let waiting = scheduler.waiting(thread.clone(), args, clock);
        scheduler.add_new(waiting);
        Ok(thread)
    })
}

pub(crate) fn wakeup(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, thread: Thread| Ok(lock(&shared).wake(&thread, None)))
}

/// `wake(thread, ...)` resumes a thread in a wait with the values.
pub(crate) fn wake(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, (thread, values): (Thread, MultiValue)| {
        Ok(lock(&shared).wake(&thread, Some(values)))
    })
}

pub(crate) fn kill(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, thread: Thread| {
        let mut scheduler = lock(&shared);
        if scheduler.is_running_thread(&thread) {
            scheduler.killed_running = true;
            return Ok(true);
        }
        Ok(scheduler.remove(&thread))
    })
}

pub(crate) fn current(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ()| Ok(lock(&shared).running()))
}

/// `now()` reads the clock of the running thread, or the default clock outside one.
pub(crate) fn now(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ()| Ok(lock(&shared).running_now()))
}

/// A native that reads the clock at index `clock`.
pub(crate) fn read_clock(lua: &Lua, shared: &Shared, clock: usize) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ()| Ok(lock(&shared).clocks[clock].now))
}

/// `adopt(thread, clock, flags, source, remaining)` for a thread noted as parked.
pub(crate) fn adopt(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(
        move |_,
              (thread, clock, flags, source, remaining): (
            Thread,
            String,
            i64,
            Option<String>,
            Option<i64>,
        )| {
            let mut scheduler = lock(&shared);
            let kind = scheduler.parked.get(&id(&thread)).cloned().ok_or_else(|| {
                mlua::Error::runtime("thread is not parked in a scheduler native")
            })?;
            let clock = scheduler
                .clock_index(&clock)
                .ok_or_else(|| mlua::Error::runtime(format!("no clock named {clock}")))?;
            let left = remaining.map(|ms| u64::try_from(ms).unwrap_or(0));
            let parking = scheduler
                .parking_of(&kind, left)
                .map_err(mlua::Error::runtime)?;
            let game = (flags, source.unwrap_or_default());
            scheduler
                .adopt(thread, clock, parking, game)
                .map_err(mlua::Error::runtime)?;
            Ok(true)
        },
    )
}
