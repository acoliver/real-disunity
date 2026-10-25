#[cfg(test)]
mod tests {
    use crate::{abi::allocation_failure, result_code, store::AllocationError};
    use worldrunner_world_simulation::{LimitKind, WorldError};

    #[test]
    fn allocation_capacity_is_a_budget_result_not_a_bad_handle() {
        let error = AllocationError::Native(WorldError::CapacityExceeded(LimitKind::Components));
        assert_eq!(allocation_failure(error).code, result_code::BUDGET);
        assert_eq!(
            allocation_failure(AllocationError::HandleCapacity).code,
            result_code::BUDGET
        );
        assert_eq!(
            allocation_failure(AllocationError::InvalidWorld).code,
            result_code::INVALID_HANDLE
        );
    }
}
