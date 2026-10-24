use super::{
    BufferUse, BufferUseKind, CompiledOperations, ContractError, RasterPass, ResourceRegistry,
    ResourceUse, TypedOperation, TypedWork,
};
use super::{buffers::BufferVersions, validation::Versions};

type Accesses = (Vec<ResourceUse>, Vec<BufferUse>);
impl ResourceRegistry {
    /// Validates the entire authored list before allocating immutable upload staging.
    pub fn compile_operations(
        &self,
        device: &wgpu::Device,
        operations: &[TypedOperation],
    ) -> Result<CompiledOperations, ContractError> {
        let mut textures = Versions::default();
        let mut buffers = BufferVersions::default();
        let mut accesses = Vec::new();
        for (index, operation) in operations.iter().enumerate() {
            if operations[..index]
                .iter()
                .any(|other| other.id == operation.id)
            {
                return Err(ContractError::Raster(operation.id));
            }
            let usage = self
                .validate_operation(device, operation, &mut textures, &mut buffers)
                .map_err(|source| ContractError::Pass {
                    pass: operation.id,
                    source: Box::new(source),
                })?;
            accesses.push(usage);
        }
        Ok(CompiledOperations {
            operations: operations.to_vec(),
            accesses,
            staging: super::operations::stage_uploads(device, operations),
        })
    }
    fn validate_operation(
        &self,
        device: &wgpu::Device,
        operation: &TypedOperation,
        textures: &mut Versions,
        buffers: &mut BufferVersions,
    ) -> Result<Accesses, ContractError> {
        validate_work(device, operation)?;
        let (mut texture_uses, mut buffer_uses) = accesses(operation)?;
        if let Some(pass) = operation.raster() {
            if pass.id != operation.id || !operation.textures.is_empty() {
                return Err(ContractError::Raster(operation.id));
            }
            self.validate_pass(device, pass, textures)?;
            texture_uses = pass.accesses();
        } else {
            texture_uses.extend(operation.textures.clone());
            validate_textures(self, device, &texture_uses, textures)?;
        }
        buffer_uses.extend(operation.buffers.clone());
        validate_buffers(self, &buffer_uses, buffers)?;
        let binding_pass = RasterPass {
            id: operation.id,
            colors: Vec::new(),
            depth: None,
            uses: texture_uses.clone(),
            bindings: Vec::new(),
        };
        for group in operation.bindings() {
            group.validate(self, &binding_pass)?;
            group.validate_buffers(self, &buffer_uses)?;
        }
        super::buffer_scopes::validate(operation)?;
        Ok((texture_uses, buffer_uses))
    }
}
fn validate_textures(
    registry: &ResourceRegistry,
    device: &wgpu::Device,
    accesses: &[ResourceUse],
    versions: &mut Versions,
) -> Result<(), ContractError> {
    for (index, usage) in accesses.iter().enumerate() {
        registry.check_view(&usage.view)?;
        super::validation::validate_usage(registry, usage)?;
        for previous in &accesses[..index] {
            if previous.view.allocation == usage.view.allocation
                && previous.view.range.overlaps(&usage.view.range)
            {
                super::validation::validate_overlap(device, previous, usage)?;
            }
        }
    }
    for usage in accesses {
        super::validation::validate_versions(registry, usage, versions)?;
    }
    Ok(())
}
fn validate_buffers(
    registry: &ResourceRegistry,
    accesses: &[BufferUse],
    versions: &mut BufferVersions,
) -> Result<(), ContractError> {
    for (index, usage) in accesses.iter().enumerate() {
        registry.check_buffer(&usage.view)?;
        for previous in &accesses[..index] {
            if previous.view.allocation == usage.view.allocation
                && super::buffers::overlaps(&previous.view.range, &usage.view.range)
                && (super::transfers::is_write(previous.access)
                    || super::transfers::is_write(usage.access))
            {
                return Err(ContractError::Hazard(usage.view.allocation));
            }
        }
    }
    for usage in accesses {
        versions.validate(registry, usage)?;
    }
    Ok(())
}
fn accesses(operation: &TypedOperation) -> Result<Accesses, ContractError> {
    let mut textures = Vec::new();
    let mut buffers = buffer_accesses(operation)?;
    match &operation.work {
        TypedWork::CopyTexture {
            source,
            destination,
        } => {
            super::transfers::texture_copy(source, destination)?;
            textures.push(source.usage(true)?);
            textures.push(destination.usage(false)?);
        }
        TypedWork::CopyBufferToTexture {
            source,
            destination,
            bytes_per_row,
            rows_per_image,
        } => {
            let spans = super::transfers::texture_buffer(
                destination,
                source,
                *bytes_per_row,
                *rows_per_image,
                false,
            )?;
            textures.push(destination.usage(false)?);
            buffers.extend(
                spans
                    .into_iter()
                    .map(|span| BufferUse::read(span, BufferUseKind::Copy)),
            );
        }
        TypedWork::CopyTextureToBuffer {
            source,
            destination,
            bytes_per_row,
            rows_per_image,
        }
        | TypedWork::ReadbackTexture {
            source,
            destination,
            bytes_per_row,
            rows_per_image,
        } => {
            let spans = super::transfers::texture_buffer(
                source,
                destination,
                *bytes_per_row,
                *rows_per_image,
                true,
            )?;
            textures.push(source.usage(true)?);
            buffers.extend(
                spans
                    .into_iter()
                    .map(|span| BufferUse::write(span, BufferUseKind::Copy)),
            );
        }
        _ => {}
    }
    Ok((textures, buffers))
}
fn buffer_accesses(operation: &TypedOperation) -> Result<Vec<BufferUse>, ContractError> {
    let mut buffers = Vec::new();
    match &operation.work {
        TypedWork::CopyBuffer {
            source,
            destination,
        }
        | TypedWork::ReadbackBuffer {
            source,
            destination,
        } => {
            super::transfers::buffer_copy(source, destination)?;
            buffers.push(BufferUse::read(source.clone(), BufferUseKind::Copy));
            buffers.push(BufferUse::write(destination.clone(), BufferUseKind::Copy));
        }
        TypedWork::Upload { destination, bytes } => {
            validate_upload(destination, bytes)?;
            buffers.push(BufferUse::write(destination.clone(), BufferUseKind::Copy));
        }
        _ => {}
    }
    Ok(buffers)
}
fn validate_work(device: &wgpu::Device, operation: &TypedOperation) -> Result<(), ContractError> {
    if operation.bindings().len() > device.limits().max_bind_groups as usize {
        return Err(ContractError::Pipeline);
    }
    match &operation.work {
        TypedWork::Compute { workgroups, .. }
            if workgroups.iter().any(|&count| {
                count == 0 || count > device.limits().max_compute_workgroups_per_dimension
            }) =>
        {
            return Err(ContractError::Dispatch(*workgroups));
        }
        TypedWork::ReadbackBuffer { destination, .. }
        | TypedWork::ReadbackTexture { destination, .. }
            if !destination
                .buffer
                .usage()
                .contains(wgpu::BufferUsages::MAP_READ) =>
        {
            return Err(ContractError::Binding(destination.allocation));
        }
        _ => {}
    }
    validate_declared_kinds(operation)?;
    validate_dynamic_limits(device, operation)
}
fn validate_dynamic_limits(
    device: &wgpu::Device,
    operation: &TypedOperation,
) -> Result<(), ContractError> {
    let dynamic_uniforms = operation
        .bindings()
        .iter()
        .flat_map(|group| &group.buffers)
        .filter(|entry| {
            entry.dynamic_offset.is_some() && entry.kind == super::BufferBindingKind::Uniform
        })
        .count();
    let dynamic_storage = operation
        .bindings()
        .iter()
        .flat_map(|group| &group.buffers)
        .filter(|entry| {
            entry.dynamic_offset.is_some()
                && matches!(entry.kind, super::BufferBindingKind::Storage(_))
        })
        .count();
    if dynamic_uniforms
        > device
            .limits()
            .max_dynamic_uniform_buffers_per_pipeline_layout as usize
        || dynamic_storage
            > device
                .limits()
                .max_dynamic_storage_buffers_per_pipeline_layout as usize
    {
        return Err(ContractError::Pipeline);
    }
    Ok(())
}

fn validate_upload(destination: &super::BufferView, bytes: &[u8]) -> Result<(), ContractError> {
    if !destination.range.start.is_multiple_of(4) || !bytes.len().is_multiple_of(4) {
        return Err(ContractError::CopyAlignment(destination.allocation));
    }
    if bytes.is_empty() || bytes.len() as u64 != destination.range.end - destination.range.start {
        return Err(ContractError::CopyExtent(destination.allocation));
    }
    Ok(())
}

fn validate_declared_kinds(operation: &TypedOperation) -> Result<(), ContractError> {
    let compute = matches!(operation.work, TypedWork::Compute { .. });
    if operation.raster().is_none()
        && let Some(usage) = operation.textures.iter().find(|usage| {
            !compute
                || !matches!(
                    usage.kind,
                    super::UseKind::Sampled | super::UseKind::Storage
                )
        })
    {
        return Err(ContractError::Binding(usage.view.allocation));
    }
    if let Some(usage) = operation.buffers.iter().find(|usage| {
        if operation.raster().is_some() {
            usage.kind == BufferUseKind::Copy
        } else {
            !compute || !matches!(usage.kind, BufferUseKind::Uniform | BufferUseKind::Storage)
        }
    }) {
        return Err(ContractError::Binding(usage.view.allocation));
    }
    Ok(())
}
