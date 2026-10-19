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
