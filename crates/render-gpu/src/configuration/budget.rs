use super::{AllocationBudget, ConfigurationError};

/// Descriptor-byte admission policy, evaluated for the complete live allocation peak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocationBudgetPolicy {
    Fixed(AllocationBudget),
    Derived {
        headroom_bytes: u64,
        granularity_bytes: u64,
        cap_bytes: Option<u64>,
    },
}

impl AllocationBudgetPolicy {
    pub fn resolve<E>(self, required: u64) -> Result<AllocationBudget, ConfigurationError<E>> {
        let budget = match self {
            Self::Fixed(budget) => budget.0,
            Self::Derived {
                headroom_bytes,
                granularity_bytes,
                cap_bytes,
            } => {
                if granularity_bytes == 0 {
                    return Err(ConfigurationError::Invalid("zero budget granularity"));
                }
                let rounded = required
                    .checked_add(headroom_bytes)
                    .and_then(|bytes| bytes.checked_next_multiple_of(granularity_bytes))
                    .ok_or(ConfigurationError::Invalid("allocation budget overflow"))?;
                cap_bytes.map_or(rounded, |cap| rounded.min(cap))
            }
        };
        if required > budget {
            return Err(ConfigurationError::Budget { required, budget });
        }
        Ok(AllocationBudget(budget))
    }
}

impl From<AllocationBudget> for AllocationBudgetPolicy {
    fn from(budget: AllocationBudget) -> Self {
        Self::Fixed(budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_rounding_cap_and_overflow_are_explicit() {
        let policy = AllocationBudgetPolicy::Derived {
            headroom_bytes: 7,
            granularity_bytes: 16,
            cap_bytes: None,
        };
        assert_eq!(policy.resolve::<()>(25).unwrap(), AllocationBudget(32));
        assert_eq!(policy.resolve::<()>(26).unwrap(), AllocationBudget(48));
        assert!(matches!(
            policy.resolve::<()>(u64::MAX),
            Err(ConfigurationError::Invalid(_))
        ));
        let cap = AllocationBudgetPolicy::Derived {
            headroom_bytes: 7,
            granularity_bytes: 16,
            cap_bytes: Some(24),
        };
        assert!(matches!(
            cap.resolve::<()>(25),
            Err(ConfigurationError::Budget {
                required: 25,
                budget: 24
            })
        ));
    }
}
