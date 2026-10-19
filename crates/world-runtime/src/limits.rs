//! Caller-supplied operational limits for runtime world preparation.

use worldrunner_world_model::ValidationLimits;

/// Complete operational budget for one runtime planning pass.
///
/// No dataset population assumptions or defaults are compiled into this type.
/// Callers must choose every bound explicitly for each planning operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimePlanLimits {
    semantic: ValidationLimits,
    max_components: usize,
    max_bindings: usize,
    max_binding_bytes: u64,
    max_total_binding_bytes: u64,
}

impl RuntimePlanLimits {
    /// Creates an explicit runtime planning budget.
    #[must_use]
    pub const fn new(
        semantic: ValidationLimits,
        max_components: usize,
        max_bindings: usize,
        max_binding_bytes: u64,
        max_total_binding_bytes: u64,
    ) -> Self {
        Self {
            semantic,
            max_components,
            max_bindings,
            max_binding_bytes,
            max_total_binding_bytes,
        }
    }

    /// Returns the aggregate semantic payload budget.
    #[must_use]
    pub const fn semantic(&self) -> &ValidationLimits {
        &self.semantic
    }

    /// Returns the maximum component count.
    #[must_use]
    pub const fn max_components(&self) -> usize {
        self.max_components
    }

    /// Returns the maximum resource-binding count.
    #[must_use]
    pub const fn max_bindings(&self) -> usize {
        self.max_bindings
    }

    /// Returns the maximum declared bytes of one binding.
    #[must_use]
    pub const fn max_binding_bytes(&self) -> u64 {
        self.max_binding_bytes
    }

    /// Returns the maximum checked sum of all declared binding bytes.
    #[must_use]
    pub const fn max_total_binding_bytes(&self) -> u64 {
        self.max_total_binding_bytes
    }
}
