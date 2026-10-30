//! The limits the budget hook enforces and the report of a thread that ran past them.
//!
//! Lua hooks belong to a thread, so the limits live in one shared [`Active`] that every hook
//! reads. The scheduler swaps in the resume limits around each resume and restores the entry
//! limits afterwards.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use mlua::{Debug, Lua, VmState};

use crate::config::{OnOverrun, ResumeBudget};

/// The deepest stack frame a report lists.
const MAX_FRAMES: usize = 30;

/// Instructions between hook fires for the budget of an entry from the host.
const ENTRY_STEP: u32 = 1024;

/// What one stretch of Lua code may spend.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    instructions: u64,
    step: u32,
    wall_time: Option<Duration>,
    on_overrun: OnOverrun,
}

impl Limits {
    /// A host entry: instructions only, and a Lua error when they run out.
    pub(crate) fn entry(instructions: u32) -> Self {
        Self {
            instructions: u64::from(instructions),
            step: ENTRY_STEP,
            wall_time: None,
            on_overrun: OnOverrun::RaiseError,
        }
    }

    pub(crate) fn resume(budget: &ResumeBudget) -> Self {
        Self {
            instructions: budget.instructions,
            step: budget.check_every.max(1),
            wall_time: Some(budget.wall_time),
            on_overrun: budget.on_overrun,
        }
    }
}

/// The thread a hook suspended, and why.
struct Stopped {
    report: String,
    thread: usize,
}

/// The error of a thread whose suspension did not take effect.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct Exceeded(String);

/// The limits in force now, the time they end, and the report of an overrun.
pub(crate) struct Active {
    limits: Limits,
    deadline: Option<Instant>,
    stopped: Option<Stopped>,
}

impl Active {
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            deadline: None,
            stopped: None,
        }
    }

    pub(crate) fn step(&self) -> u32 {
        self.limits.step
    }

    /// Starts a resume under `limits`, with the wall time counted from now.
    pub(crate) fn begin(&mut self, limits: Limits) {
        self.deadline = limits.wall_time.map(|time| Instant::now() + time);
        self.limits = limits;
        self.stopped = None;
    }

    /// Ends the resume: back to the `entry` limits, returning the report of an overrun.
    pub(crate) fn finish(&mut self, entry: Limits) -> Option<String> {
        self.limits = entry;
        self.deadline = None;
        self.stopped.take().map(|stopped| stopped.report)
    }

    /// What the hook does after `spent` instructions.
    pub(crate) fn check(&mut self, lua: &Lua, at: &Debug, spent: u64) -> mlua::Result<VmState> {
        let Some(reason) = self.overrun(spent) else {
            return Ok(VmState::Continue);
        };
        match self.limits.on_overrun {
            OnOverrun::RaiseError => Err(mlua::Error::RuntimeError(reason)),
            OnOverrun::EndThreadWithReport => self.suspend(lua, at),
        }
    }

    fn overrun(&self, spent: u64) -> Option<String> {
        if spent > self.limits.instructions {
            return Some(format!(
                "instruction budget of {} exceeded",
                self.limits.instructions
            ));
        }
        let (deadline, time) = (self.deadline?, self.limits.wall_time?);
        (Instant::now() > deadline)
            .then(|| format!("resume wall time of {} ms exceeded", time.as_millis()))
    }

    /// Suspends the running thread, because an error raised here would be caught by any
    /// `pcall` the thread is looping in. The first overrun records the report. A later one on
    /// another thread is the thread that resumed it, and is suspended the same way. A later
    /// one on the same thread means the suspension did not take, as inside a C call such as a
    /// sort comparator, so the thread is stopped by an error instead.
    fn suspend(&mut self, lua: &Lua, at: &Debug) -> mlua::Result<VmState> {
        let thread = lua.current_thread().to_pointer() as usize;
        match &self.stopped {
            Some(stopped) if stopped.thread == thread => {
                Err(mlua::Error::external(Exceeded(stopped.report.clone())))
            }
            Some(_) => Ok(VmState::Yield),
            None => {
                let report = report(lua, at);
                self.stopped = Some(Stopped { report, thread });
                Ok(VmState::Yield)
            }
        }
    }
}

fn place(debug: &Debug) -> String {
    let source = debug.source();
    let file = source.short_src.as_deref().unwrap_or("?");
    format!("{file}:{}", debug.curr_line())
}

/// `budget exceeded at <file:line>` followed by the stack of the thread, innermost first.
fn report(lua: &Lua, running: &Debug) -> String {
    let mut text = format!("budget exceeded at {}\nstack traceback:", place(running));
    let _ = write!(text, "\n\t{}: in {}", place(running), describe(running));
    for level in 1..MAX_FRAMES {
        let Some(frame) = lua.inspect_stack(level) else {
            return text;
        };
        let _ = write!(text, "\n\t{}: in {}", place(&frame), describe(&frame));
    }
    text.push_str("\n\t...");
    text
}

fn describe(debug: &Debug) -> String {
    let names = debug.names();
    let source = debug.source();
    match (names.name, source.what) {
        (Some(name), _) => format!("function '{name}'"),
        (None, "main") => "main chunk".to_owned(),
        (None, _) => format!(
            "function <{}:{}>",
            source.short_src.as_deref().unwrap_or("?"),
            source.line_defined.unwrap_or(0)
        ),
    }
}
