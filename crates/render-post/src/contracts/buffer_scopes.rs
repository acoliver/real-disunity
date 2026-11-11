use std::collections::BTreeMap;

use super::{AccessMode, BufferUse, BufferUseKind, ContractError, TypedOperation};

/// Writable storage is an exclusive usage of its buffer: no binding or geometry read of the same
/// allocation may share the operation with it. Several bindings with that same usage are legal
/// when their byte hazards are compatible, which the buffer checks decide.
///
/// Each allocation is seen once for every usage, so an operation of many draws costs a pass over
/// them and not a comparison of every pair.
pub(super) fn validate(operation: &TypedOperation) -> Result<(), ContractError> {
    let mut seen = BTreeMap::new();
    for group in operation.bindings() {
        for entry in &group.buffers {
            note(&mut seen, &entry.usage()?)?;
        }
    }
    for usage in operation
        .buffers
        .iter()
        .filter(|usage| matches!(usage.kind, BufferUseKind::Vertex | BufferUseKind::Index))
    {
        note(&mut seen, usage)?;
    }
    Ok(())
}

/// Notes `usage` against the usages of its allocation seen so far: whether any was writable and
/// whether any was not.
fn note(
    seen: &mut BTreeMap<super::AllocationKey, (bool, bool)>,
    usage: &BufferUse,
) -> Result<(), ContractError> {
    let writable = usage.kind == BufferUseKind::Storage && usage.access != AccessMode::Read;
    let (any_writable, any_other) = seen.entry(usage.view.allocation).or_default();
    if writable {
        *any_writable = true;
    } else {
        *any_other = true;
    }
    if *any_writable && *any_other {
        return Err(ContractError::BufferUsage(usage.view.allocation));
    }
    Ok(())
}
