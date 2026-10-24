use super::{
    AccessMode, AllocationKey, CompiledOperations, ContractError, ResourceRegistry, TypedOperation,
};
use worldrunner_render_gpu::submission::capture::{CaptureWrites, WriteKey};

/// Both segments are validated together, including cross-segment producer versions.
/// Splitting does not rename or reorder any operation's stage identity.
pub struct CompiledCapture {
    prefix: CompiledOperations,
    suffix: CompiledOperations,
    writes: CaptureWrites,
}
impl CompiledCapture {
    pub const fn prefix(&self) -> &CompiledOperations {
        &self.prefix
    }
    pub const fn suffix(&self) -> &CompiledOperations {
        &self.suffix
    }
    pub const fn writes(&self) -> &CaptureWrites {
        &self.writes
    }
}
impl ResourceRegistry {
    pub fn compile_capture(
        &self,
        device: &wgpu::Device,
        prefix: &[TypedOperation],
        suffix: &[TypedOperation],
        suffix_owned: &[AllocationKey],
    ) -> Result<CompiledCapture, ContractError> {
        let operations: Vec<_> = prefix.iter().chain(suffix).cloned().collect();
        let mut compiled = self.compile_operations(device, &operations)?;
        let suffix = CompiledOperations {
            operations: compiled.operations.split_off(prefix.len()),
            accesses: compiled.accesses.split_off(prefix.len()),
            staging: compiled.staging.split_off(prefix.len()),
        };
        let writes = CaptureWrites::new(
            compiled.write_keys(),
            suffix.write_keys(),
            suffix_owned.iter().copied().map(write_key),
        )
        .map_err(ContractError::CaptureWrite)?;
        Ok(CompiledCapture {
            prefix: compiled,
            suffix,
            writes,
        })
    }
}
pub(super) fn write_key(key: AllocationKey) -> WriteKey {
    WriteKey::Resource {
        id: key.id(),
        generation: key.generation(),
    }
}
pub(super) fn validate_writes(
    encoder: &wgpu::CommandEncoder,
    keys: impl IntoIterator<Item = WriteKey>,
) -> Result<(), ContractError> {
    use worldrunner_render_gpu::submission::Publications;
    if Publications::current().is_some() {
        let publications =
            Publications::for_encoder(encoder).map_err(ContractError::Transaction)?;
        for key in keys {
            publications
                .write(key)
                .map_err(ContractError::CaptureWrite)?;
        }
    }
    Ok(())
}
impl CompiledOperations {
    pub(super) fn write_keys(&self) -> Vec<WriteKey> {
        self.accesses
            .iter()
            .flat_map(|(textures, buffers)| {
                textures
                    .iter()
                    .filter(|usage| usage.access != AccessMode::Read)
                    .map(|usage| write_key(usage.view.allocation()))
                    .chain(
                        buffers
                            .iter()
                            .filter(|usage| usage.access != AccessMode::Read)
                            .map(|usage| write_key(usage.view.allocation())),
                    )
            })
            .collect()
    }
}
