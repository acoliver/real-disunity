use crate::ViewerError;
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationBudgetPolicy};

/// Fixed presentation descriptor-byte limit, including CPU capture storage and
/// concurrently resident renderer generations. Zero fails option validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationBudget(pub u64);

impl Default for PresentationBudget {
    fn default() -> Self {
        Self(1024 * 1024 * 1024)
    }
}

/// Admission policy evaluated at creation and every configuration replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationBudgetPolicy {
    Fixed(PresentationBudget),
    /// Round the live replacement peak plus headroom up to a positive granularity.
    /// The cap clamps the budget; a peak above the cap returns a typed budget error.
    Derived {
        headroom_bytes: u64,
        granularity_bytes: u64,
        cap_bytes: Option<u64>,
    },
}

impl Default for PresentationBudgetPolicy {
    fn default() -> Self {
        PresentationBudget::default().into()
    }
}

impl From<PresentationBudget> for PresentationBudgetPolicy {
    fn from(budget: PresentationBudget) -> Self {
        Self::Fixed(budget)
    }
}

impl PresentationBudgetPolicy {
    pub(crate) fn allocation_policy(self) -> AllocationBudgetPolicy {
        match self {
            Self::Fixed(budget) => AllocationBudgetPolicy::Fixed(AllocationBudget(budget.0)),
            Self::Derived {
                headroom_bytes,
                granularity_bytes,
                cap_bytes,
            } => AllocationBudgetPolicy::Derived {
                headroom_bytes,
                granularity_bytes,
                cap_bytes,
            },
        }
    }
    pub(crate) fn validate(self) -> Result<(), ViewerError> {
        match self {
            Self::Fixed(PresentationBudget(0)) => Err(ViewerError::InvalidOptions(
                "presentation allocation budget must be positive",
            )),
            Self::Derived {
                granularity_bytes: 0,
                ..
            } => Err(ViewerError::InvalidOptions(
                "presentation budget granularity must be positive",
            )),
            Self::Derived {
                cap_bytes: Some(0), ..
            } => Err(ViewerError::InvalidOptions(
                "presentation budget cap must be positive",
            )),
            _ => Ok(()),
        }
    }
}
