//! Caller-supplied limits for render world plan minting.

use worldrunner_world_runtime::RuntimePlanLimits;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderWorldLimits {
    runtime: RuntimePlanLimits,
    max_prepared_components: usize,
    max_prepared_resources: usize,
}

impl RenderWorldLimits {
    /// Creates one explicit render planning budget.
    #[must_use]
    pub const fn new(
        runtime: RuntimePlanLimits,
        max_prepared_components: usize,
        max_prepared_resources: usize,
    ) -> Self {
        Self {
            runtime,
            max_prepared_components,
            max_prepared_resources,
        }
    }

    /// Returns the runtime preparation budget.
    #[must_use]
    pub const fn runtime(&self) -> &RuntimePlanLimits {
        &self.runtime
    }

    /// Returns the maximum prepared component contribution count.
    #[must_use]
    pub const fn max_prepared_components(&self) -> usize {
        self.max_prepared_components
    }

    /// Returns the maximum prepared resource contribution count.
    #[must_use]
    pub const fn max_prepared_resources(&self) -> usize {
        self.max_prepared_resources
    }
}
