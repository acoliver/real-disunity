//! The Rust natives of the scheduler.

use std::sync::{Arc, Mutex};

use mlua::{Function, Lua, MultiValue, Thread, Value};

use super::state::{Origin, Scheduler, id};

pub(crate) type Shared = Arc<Mutex<Scheduler>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Scheduler> {
    shared.lock().expect("scheduler")
}

/// `spawn(f, ...)` on the clock at index `clock`, the new thread carrying `flags`.
pub(crate) fn spawn(
    lua: &Lua,
    shared: &Shared,
    clock: usize,
    flags: i64,
) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |lua, (function, args): (Function, MultiValue)| {
        let thread = lua.create_thread(function.clone())?;
        let mut scheduler = lock(&shared);
        let mut origin = Origin::new(Some((function, args.clone())));
        origin.flags = flags;
        scheduler.origins.insert(id(&thread), origin);
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

/// `wake(thread, ...)` for the wait kind at index `kind`: a thread in another kind of wait, one
/// already woken and any other value is left alone and the native returns false.
pub(crate) fn wake_kind(lua: &Lua, shared: &Shared, kind: usize) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, (thread, values): (Value, MultiValue)| {
        Ok(thread_of(&thread).is_some_and(|thread| lock(&shared).wake_kind(thread, kind, values)))
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

fn thread_of(value: &Value) -> Option<&Thread> {
    match value {
        Value::Thread(thread) => Some(thread),
        _ => None,
    }
}

/// `get(thread)` returns the game's flags on a scheduler thread, nil for any other value.
pub(crate) fn get_flags(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, thread: Value| {
        Ok(thread_of(&thread).and_then(|thread| lock(&shared).flags_of(thread)))
    })
}

/// `set(thread, flags)` adds bits to a thread's flags; anything else is ignored.
pub(crate) fn set_flags(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, (thread, flags): (Value, Option<i64>)| {
        if let Some(thread) = thread_of(&thread) {
            lock(&shared).add_flags(thread, flags.unwrap_or(0));
        }
        Ok(())
    })
}

/// `has(thread, mask)` is true for a scheduler thread that carries every bit of `mask`.
pub(crate) fn has_flags(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, (thread, mask): (Value, Option<i64>)| {
        let mask = mask.unwrap_or(0);
        let flags = thread_of(&thread).and_then(|thread| lock(&shared).flags_of(thread));
        Ok(flags.is_some_and(|flags| flags & mask == mask))
    })
}

/// `set_source(thread, name)` tags a scheduler thread; anything else is ignored.
pub(crate) fn set_source(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, (thread, name): (Value, String)| {
        if let Some(thread) = thread_of(&thread) {
            lock(&shared).set_source(thread, &name);
        }
        Ok(())
    })
}

/// `clock_of(thread)` returns the name of a scheduler thread's clock, nil for any other value.
pub(crate) fn clock_of(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, thread: Value| {
        Ok(thread_of(&thread).and_then(|thread| lock(&shared).clock_name_of(thread)))
    })
}

/// `set_clock(ms)` sets the clock at index `clock`; nil and negative times are 0.
pub(crate) fn set_clock(lua: &Lua, shared: &Shared, clock: usize) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ms: Option<i64>| {
        let ms = u64::try_from(ms.unwrap_or(0)).unwrap_or(0);
        lock(&shared).clocks[clock].now = ms;
        Ok(())
    })
}

/// `interrupt()` ends the advance in progress once the running thread yields.
pub(crate) fn interrupt(lua: &Lua, shared: &Shared) -> mlua::Result<Function> {
    let shared = Arc::clone(shared);
    lua.create_function(move |_, ()| {
        lock(&shared).interrupted = true;
        Ok(())
    })
}
