use std::ops::Range;

use super::{AccessMode, AllocationKey, ContractError, ResourceLifetime, ResourceRegistry};

#[derive(Clone, Debug)]
pub struct BufferView {
    pub(super) allocation: AllocationKey,
    pub(super) range: Range<u64>,
    pub(super) version: u32,
    pub(super) buffer: wgpu::Buffer,
}
impl BufferView {
    pub const fn allocation(&self) -> AllocationKey {
        self.allocation
    }
    pub fn range(&self) -> Range<u64> {
        self.range.clone()
    }
    pub const fn version(&self) -> u32 {
        self.version
    }
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferUseKind {
    Uniform,
    Storage,
    Vertex,
    Index,
    Copy,
}
#[derive(Clone, Debug)]
pub struct BufferUse {
    pub view: BufferView,
    pub before: Option<u32>,
    pub access: AccessMode,
    pub kind: BufferUseKind,
}
impl BufferUse {
    pub fn read(view: BufferView, kind: BufferUseKind) -> Self {
        Self {
            before: Some(view.version),
            view,
            access: AccessMode::Read,
            kind,
        }
    }
    pub fn write(view: BufferView, kind: BufferUseKind) -> Self {
        Self {
            before: None,
            view,
            access: AccessMode::Write,
            kind,
        }
    }
    pub fn read_write(view: BufferView, before: u32, kind: BufferUseKind) -> Self {
        Self {
            before: Some(before),
            view,
            access: AccessMode::ReadWrite,
            kind,
        }
    }
}
pub(super) struct BufferAllocation {
    pub key: AllocationKey,
    pub buffer: wgpu::Buffer,
    pub lifetime: ResourceLifetime,
    pub initial: Option<u32>,
}
impl ResourceRegistry {
    pub fn import_buffer(
        &mut self,
        buffer: &wgpu::Buffer,
        lifetime: ResourceLifetime,
        initial: Option<u32>,
    ) -> Result<AllocationKey, ContractError> {
        if let Some((key, _)) = self.retired_buffers.iter().find(|(_, old)| old == buffer) {
            return Err(ContractError::StaleAllocation(*key));
        }
        if let Some(entry) = self.buffers.iter().find(|entry| entry.buffer == *buffer) {
            if (entry.lifetime, entry.initial) != (lifetime, initial) {
                return Err(ContractError::Identity(entry.key));
            }
            return Ok(entry.key);
        }
        let key = super::registry::next_key();
        self.buffers.push(BufferAllocation {
            key,
            buffer: buffer.clone(),
            lifetime,
            initial,
        });
        Ok(key)
    }
    pub fn replace_buffer(
        &mut self,
        key: AllocationKey,
        buffer: &wgpu::Buffer,
    ) -> Result<AllocationKey, ContractError> {
        if self.buffer_allocation(key)?.lifetime == ResourceLifetime::ImportedReadOnly {
            return Err(ContractError::ReadOnlyReplacement(key));
        }
        if self.buffers.iter().any(|entry| entry.buffer == *buffer)
            || self.retired_buffers.iter().any(|(_, old)| old == buffer)
        {
            return Err(ContractError::Identity(key));
        }
        let entry = self
            .buffers
            .iter_mut()
            .find(|entry| entry.key == key)
            .unwrap();
        entry.key.generation = key
            .generation
            .checked_add(1)
            .ok_or(ContractError::Identity(key))?;
        self.retired_buffers.push((key, entry.buffer.clone()));
        entry.buffer = buffer.clone();
        entry.initial = None;
        Ok(entry.key)
    }
    pub(super) fn buffer_allocation(
        &self,
        key: AllocationKey,
    ) -> Result<&BufferAllocation, ContractError> {
        self.buffers
            .iter()
            .find(|entry| entry.key == key)
            .ok_or(ContractError::StaleAllocation(key))
    }
    pub fn buffer_view(
        &self,
        key: AllocationKey,
        range: Range<u64>,
        version: u32,
    ) -> Result<BufferView, ContractError> {
        let entry = self.buffer_allocation(key)?;
        if range.start >= range.end || range.end > entry.buffer.size() {
            return Err(ContractError::Range(key));
        }
        Ok(BufferView {
            allocation: key,
            range,
            version,
            buffer: entry.buffer.clone(),
        })
    }
    pub fn check_buffer(&self, view: &BufferView) -> Result<(), ContractError> {
        let entry = self.buffer_allocation(view.allocation)?;
        if entry.buffer != view.buffer {
            return Err(ContractError::Identity(view.allocation));
        }
        if view.range.start >= view.range.end || view.range.end > entry.buffer.size() {
            return Err(ContractError::Range(view.allocation));
        }
        Ok(())
    }
}

pub(super) fn overlaps(a: &Range<u64>, b: &Range<u64>) -> bool {
    a.start < b.end && b.start < a.end
}

struct BufferCell {
    allocation: AllocationKey,
    range: Range<u64>,
    version: u32,
}
#[derive(Default)]
pub(super) struct BufferVersions(Vec<BufferCell>);
impl BufferVersions {
    pub fn validate(
        &mut self,
        registry: &ResourceRegistry,
        usage: &BufferUse,
    ) -> Result<(), ContractError> {
        registry.check_buffer(&usage.view)?;
        let allocation = registry.buffer_allocation(usage.view.allocation)?;
        validate_usage(allocation, usage)?;
        let mut boundaries = vec![usage.view.range.start, usage.view.range.end];
        for cell in &self.0 {
            if cell.allocation == allocation.key && overlaps(&cell.range, &usage.view.range) {
                boundaries.extend([
                    cell.range.start.max(usage.view.range.start),
                    cell.range.end.min(usage.view.range.end),
                ]);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let range = pair[0]..pair[1];
            let current = self
                .0
                .iter()
                .rev()
                .find(|cell| {
                    cell.allocation == allocation.key
                        && cell.range.start <= range.start
                        && cell.range.end >= range.end
                })
                .map_or(allocation.initial, |cell| Some(cell.version));
            if usage.access != AccessMode::Write && (current.is_none() || current != usage.before) {
                return Err(ContractError::Producer(allocation.key));
            }
            if usage.access == AccessMode::Read {
                if usage.before != Some(usage.view.version) {
                    return Err(ContractError::Producer(allocation.key));
                }
            } else {
                if current.is_some_and(|version| usage.view.version <= version) {
                    return Err(ContractError::Producer(allocation.key));
                }
                self.0.push(BufferCell {
                    allocation: allocation.key,
                    range,
                    version: usage.view.version,
                });
            }
        }
        Ok(())
    }
}
fn validate_usage(allocation: &BufferAllocation, usage: &BufferUse) -> Result<(), ContractError> {
    let writes = usage.access != AccessMode::Read;
    if writes && allocation.lifetime == ResourceLifetime::ImportedReadOnly {
        return Err(ContractError::ReadOnly(allocation.key));
    }
    let required = match (usage.kind, usage.access) {
        (BufferUseKind::Uniform, AccessMode::Read) => wgpu::BufferUsages::UNIFORM,
        (BufferUseKind::Storage, _) => wgpu::BufferUsages::STORAGE,
        (BufferUseKind::Vertex, AccessMode::Read) => wgpu::BufferUsages::VERTEX,
        (BufferUseKind::Index, AccessMode::Read) => wgpu::BufferUsages::INDEX,
        (BufferUseKind::Copy, AccessMode::Read) => wgpu::BufferUsages::COPY_SRC,
        (BufferUseKind::Copy, AccessMode::Write) => wgpu::BufferUsages::COPY_DST,
        _ => return Err(ContractError::Binding(allocation.key)),
    };
    if !allocation.buffer.usage().contains(required)
        || (usage.access == AccessMode::Write) != usage.before.is_none()
    {
        return Err(ContractError::Binding(allocation.key));
    }
    Ok(())
}
