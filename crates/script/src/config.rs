//! Host configuration. Budgets and seeds are configuration, not constants
//! buried in the host, so a test can tighten a budget without patching.

use std::time::Duration;

/// Everything the host needs at construction time.
#[derive(Debug, Clone)]
pub struct ScriptConfig {
    /// Instruction budget per hook fire, and per load, boot, restore or evaluation entry of a
    /// game's scripts. Exceeding it halts the hook with
    /// [`ScriptError::BudgetExceeded`](crate::ScriptError::BudgetExceeded)
    /// instead of hanging the frame. A boot that runs billions of instructions raises this;
    /// scheduler threads use it too unless `resume_budget` is set.
    pub instruction_budget: u32,
    /// The budget of each scheduler thread resume, separate from the budget of loading. `None`
    /// gives a resume `instruction_budget` instructions and raises an error when it runs out.
    pub resume_budget: Option<ResumeBudget>,
    /// Master seed for `worldrunner.random` streams. Two hosts with the same
    /// seed and the same scripts produce the same random sequence.
    pub seed: u64,
}

impl Default for ScriptConfig {
    fn default() -> Self {
        Self {
            instruction_budget: 1_000_000,
            resume_budget: None,
            seed: 0,
        }
    }
}

/// What happens to a scheduler thread that runs past its [`ResumeBudget`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnOverrun {
    /// A Lua error is raised in the thread. A `pcall` in the thread can catch it, so a loop
    /// that calls `pcall` can outlast the budget.
    #[default]
    RaiseError,
    /// The hook suspends the thread, which no `pcall` can catch. The thread ends and its
    /// [`ThreadFailure`](crate::ThreadFailure) message starts with `budget exceeded at
    /// <file:line>`, followed by the thread's stack.
    EndThreadWithReport,
}

/// The limits of one resume of a scheduler thread, from the wait it wakes at to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeBudget {
    /// Instructions a resume may run; every resume starts again from zero. Coroutines the
    /// thread resumes count into the same budget.
    pub instructions: u64,
    /// The hook runs every this many instructions, so the limits are enforced to within one
    /// step. A step of zero is treated as one instruction.
    pub check_every: u32,
    /// Wall time a resume may take, whatever the number of instructions it spent. Natives and
    /// error handling can run for a long time on few instructions. The time is checked at each
    /// step, so it is enforced as often as the thread runs Lua code.
    pub wall_time: Duration,
    /// What happens to a thread that runs past either limit.
    pub on_overrun: OnOverrun,
}

impl Default for ResumeBudget {
    /// 50 million instructions checked every 100,000, 20 seconds of wall time, and the thread
    /// ends with a report. A healthy resume runs from one wait to the next, far below this.
    fn default() -> Self {
        Self {
            instructions: 50_000_000,
            check_every: 100_000,
            wall_time: Duration::from_secs(20),
            on_overrun: OnOverrun::EndThreadWithReport,
        }
    }
}

/// How `/` divides in the scripts a [`GameScripts`](crate::GameScripts) loads. A host selects it
/// with [`GameScripts::set_division`](crate::GameScripts::set_division) before any chunk loads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DivisionSemantics {
    /// Lua 5.4: `/` always gives a float.
    #[default]
    Stock,
    /// The semantics of Lua 5.3 builds that divide two integers as integers. Two integers give
    /// the quotient truncated toward zero, a zero integer divisor raises a Lua error, and any
    /// other operands divide as in stock Lua.
    IntegerTruncating,
}

/// What loading does with a chunk the resumable rewrite refuses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RefusedChunks {
    /// The load fails with an error naming the chunk and the reason.
    #[default]
    Fail,
    /// The chunk loads as plain text, with the division semantics applied, and is listed by
    /// [`GameScripts::plain_chunks`](crate::GameScripts::plain_chunks). A thread running its code
    /// cannot be saved.
    LoadPlain,
}

/// What loading does when a chunk name is reused for a different text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NameCollisions {
    /// The second load fails, so the first chunk's tree is never replaced.
    #[default]
    Reject,
    /// The second text loads as plain text and is listed by
    /// [`GameScripts::plain_chunks`](crate::GameScripts::plain_chunks). The first chunk's tree
    /// is kept.
    LoadPlain,
}
