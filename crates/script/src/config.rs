//! Host configuration. Budgets and seeds are configuration, not constants
//! buried in the host, so a test can tighten a budget without patching.

/// Everything the host needs at construction time.
#[derive(Debug, Clone)]
pub struct ScriptConfig {
    /// Instruction budget per hook fire. Exceeding it halts the hook with
    /// [`ScriptError::BudgetExceeded`](crate::ScriptError::BudgetExceeded)
    /// instead of hanging the frame.
    pub instruction_budget: u32,
    /// Master seed for `worldrunner.random` streams. Two hosts with the same
    /// seed and the same scripts produce the same random sequence.
    pub seed: u64,
}

impl Default for ScriptConfig {
    fn default() -> Self {
        Self {
            instruction_budget: 1_000_000,
            seed: 0,
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
