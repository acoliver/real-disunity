use super::{AccessMode, BufferUseKind, ContractError, TypedOperation};

pub(super) fn validate(operation: &TypedOperation) -> Result<(), ContractError> {
    let mut uses = Vec::new();
    for group in operation.bindings() {
        for entry in &group.buffers {
            uses.push(entry.usage()?);
        }
    }
    uses.extend(
        operation
            .buffers
            .iter()
            .filter(|usage| matches!(usage.kind, BufferUseKind::Vertex | BufferUseKind::Index))
            .cloned(),
    );
    for (index, usage) in uses.iter().enumerate() {
        for previous in &uses[..index] {
            if previous.view.allocation != usage.view.allocation {
                continue;
            }
            let writable = |usage: &super::BufferUse| {
                usage.kind == BufferUseKind::Storage && usage.access != AccessMode::Read
            };
            // Writable storage is an exclusive usage, but multiple bindings with
            // that same usage are legal when their byte hazards are compatible.
            if writable(previous) != writable(usage) {
                return Err(ContractError::BufferUsage(usage.view.allocation));
            }
        }
    }
    Ok(())
}
