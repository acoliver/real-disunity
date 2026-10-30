//! Keeps the instruction budget on whichever coroutine is running.
//!
//! mlua runs its hook on one thread at a time and removes it from any other thread it fires on,
//! so a coroutine would otherwise run without a budget. Before a coroutine runs, the hook moves
//! to it; when the coroutine yields or ends, the hook returns to the code that resumed it. The
//! scheduler does this for its threads, and `coroutine.resume` and `coroutine.wrap` are replaced
//! with versions that do it for the coroutines scripts create. All of them count into the same
//! budget, which each entry from the engine resets. A resume of a scheduler thread can run under
//! its own limits instead, see [`Budget::resume_scheduled`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mlua::{ChunkMode, Function, HookTriggers, Lua, MultiValue, Table, Thread, Value, VmState};

use crate::config::ResumeBudget;
use crate::error::ScriptError;
use crate::overrun::{Active, Limits};

/// The budget hook and the stack of coroutines it has been moved to.
#[derive(Clone)]
pub(crate) struct Budget {
    entry: Limits,
    resume: Option<Limits>,
    counter: Arc<AtomicU64>,
    active: Arc<Mutex<Active>>,
    /// The resumed coroutines, innermost last; empty while the main state runs.
    stack: Arc<Mutex<Vec<Thread>>>,
}

impl Budget {
    pub(crate) fn new(budget: u32, resume: Option<&ResumeBudget>, counter: Arc<AtomicU64>) -> Self {
        let entry = Limits::entry(budget);
        Self {
            entry,
            resume: resume.map(Limits::resume),
            counter,
            active: Arc::new(Mutex::new(Active::new(entry))),
            stack: Arc::default(),
        }
    }

    fn active(&self) -> MutexGuard<'_, Active> {
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn triggers(&self) -> HookTriggers {
        HookTriggers::new().every_nth_instruction(self.active().step())
    }

    fn hook(&self) -> impl Fn(&Lua, mlua::Debug) -> mlua::Result<VmState> + Send + 'static {
        let (counter, active) = (Arc::clone(&self.counter), Arc::clone(&self.active));
        move |lua, debug| {
            let mut active = active.lock().unwrap_or_else(PoisonError::into_inner);
            let step = u64::from(active.step());
            let spent = counter.fetch_add(step, Ordering::Relaxed) + step;
            active.check(lua, &debug, spent)
        }
    }

    /// Hooks the main state; called once, and again whenever the hook returns to it.
    pub(crate) fn hook_main(&self, lua: &Lua) {
        lua.set_hook(self.triggers(), self.hook());
    }

    /// Resumes a scheduler thread under the resume budget, if the configuration has one. The
    /// report is set when the thread ran past the budget and was suspended by the hook; the
    /// thread has to be ended then.
    pub(crate) fn resume_scheduled(
        &self,
        lua: &Lua,
        thread: &Thread,
        args: MultiValue,
    ) -> (mlua::Result<MultiValue>, Option<String>) {
        let Some(limits) = self.resume else {
            return (self.resume(lua, thread, args), None);
        };
        self.active().begin(limits);
        let result = self.resume(lua, thread, args);
        let report = self.active().finish(self.entry);
        self.hook_main(lua);
        (result, report)
    }

    /// Resumes `thread` with the hook on it, then hands the hook back to the resumer.
    pub(crate) fn resume(
        &self,
        lua: &Lua,
        thread: &Thread,
        args: MultiValue,
    ) -> mlua::Result<MultiValue> {
        self.stack.lock().expect("hook stack").push(thread.clone());
        thread.set_hook(self.triggers(), self.hook());
        let result = thread.resume::<MultiValue>(args);
        let outer = {
            let mut stack = self.stack.lock().expect("hook stack");
            stack.pop();
            stack.last().cloned()
        };
        match outer {
            Some(outer) => outer.set_hook(self.triggers(), self.hook()),
            None => self.hook_main(lua),
        }
        result
    }
}

/// `coroutine.wrap` built on the replaced `resume`, raising errors as Lua's does.
const WRAP: &str = r"
local create, resume, pack, unpack, error = ...
return function(f)
    local co = create(f)
    return function(...)
        local results = pack(resume(co, ...))
        if results[1] then
            return unpack(results, 2, results.n)
        end
        error(results[2], 0)
    end
end
";

/// Replaces `coroutine.resume` and `coroutine.wrap` in the shared namespace.
pub(crate) fn install(lua: &Lua, budget: &Budget) -> Result<(), ScriptError> {
    let coroutine: Table = lua.globals().get("coroutine")?;
    let hooked = budget.clone();
    let resume = lua.create_function(move |lua, (thread, args): (Thread, MultiValue)| {
        let mut results = MultiValue::new();
        match hooked.resume(lua, &thread, args) {
            Ok(values) => {
                results.push_back(Value::Boolean(true));
                results.extend(values);
            }
            Err(error) => {
                results.push_back(Value::Boolean(false));
                results.push_back(error_value(lua, error)?);
            }
        }
        Ok(results)
    })?;
    let table: Table = lua.globals().get("table")?;
    let make: Function = lua
        .load(WRAP)
        .set_name("=coroutine.wrap")
        .set_mode(ChunkMode::Text)
        .into_function()?;
    let wrap: Function = make.call((
        coroutine.get::<Function>("create")?,
        resume.clone(),
        table.get::<Function>("pack")?,
        table.get::<Function>("unpack")?,
        lua.globals().get::<Function>("error")?,
    ))?;
    coroutine.set("resume", resume)?;
    coroutine.set("wrap", wrap)?;
    Ok(())
}

/// The message a failed coroutine raised, as Lua's own `coroutine.resume` returns it: without
/// the stack traceback mlua appends. Error values that are not strings arrive as their text.
fn error_value(lua: &Lua, error: mlua::Error) -> mlua::Result<Value> {
    match error {
        mlua::Error::RuntimeError(message) => {
            let message = message
                .split_once("\nstack traceback:")
                .map_or(message.as_str(), |(message, _)| message);
            Ok(Value::String(lua.create_string(message)?))
        }
        mlua::Error::CallbackError { cause, .. } => error_value(lua, (*cause).clone()),
        mlua::Error::MemoryError(_) => Err(error),
        other => Ok(Value::String(lua.create_string(other.to_string())?)),
    }
}
