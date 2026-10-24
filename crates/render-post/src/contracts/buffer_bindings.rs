use super::{
    AccessMode, BufferUse, BufferUseKind, BufferView, CheckedBindings, ContractError,
    ResourceRegistry, TextureBinding,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferBindingKind {
    Uniform,
    Storage(AccessMode),
}
#[derive(Clone, Debug)]
pub struct BufferBinding {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    /// Base byte range. Dynamic offsets shift both ends to the effective range.
    pub view: BufferView,
    pub kind: BufferBindingKind,
    pub dynamic_offset: Option<u32>,
}
impl BufferBinding {
    pub fn effective(&self) -> Result<BufferView, ContractError> {
        let offset = u64::from(self.dynamic_offset.unwrap_or(0));
        let mut view = self.view.clone();
        view.range = view
            .range
            .start
            .checked_add(offset)
            .ok_or(ContractError::DynamicOffset(view.allocation))?
            ..view
                .range
                .end
                .checked_add(offset)
                .ok_or(ContractError::DynamicOffset(view.allocation))?;
        Ok(view)
    }
    pub(super) fn usage(&self) -> Result<BufferUse, ContractError> {
        let view = self.effective()?;
        Ok(match self.kind {
            BufferBindingKind::Uniform => BufferUse::read(view, BufferUseKind::Uniform),
            BufferBindingKind::Storage(AccessMode::Read) => {
                BufferUse::read(view, BufferUseKind::Storage)
            }
            BufferBindingKind::Storage(AccessMode::Write) => {
                BufferUse::write(view, BufferUseKind::Storage)
            }
            BufferBindingKind::Storage(AccessMode::ReadWrite) => BufferUse {
                view,
                before: None,
                access: AccessMode::ReadWrite,
                kind: BufferUseKind::Storage,
            },
        })
    }
}
impl CheckedBindings {
    pub fn build_with_buffers(
        device: &wgpu::Device,
        registry: &ResourceRegistry,
        textures: &[TextureBinding],
        buffers: &[BufferBinding],
    ) -> Result<Self, ContractError> {
        let mut layout_entries = texture_layouts(device, registry, textures)?;
        let mut sorted = buffers.to_vec();
        sorted.sort_by_key(|entry| entry.binding);
        for entry in &sorted {
            check_buffer_binding(device, registry, entry)?;
            if layout_entries
                .iter()
                .any(|old| old.binding == entry.binding)
            {
                return Err(ContractError::BindingLayout(entry.view.allocation));
            }
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: entry.binding,
                visibility: entry.visibility,
                ty: buffer_type(entry),
                count: None,
            });
        }
        check_buffer_limits(device, &sorted)?;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("checked resource layout"),
            entries: &layout_entries,
        });
        let mut entries: Vec<_> = textures
            .iter()
            .map(|entry| wgpu::BindGroupEntry {
                binding: entry.binding,
                resource: wgpu::BindingResource::TextureView(entry.view.view()),
            })
            .collect();
        entries.extend(sorted.iter().map(|entry| wgpu::BindGroupEntry {
            binding: entry.binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &entry.view.buffer,
                offset: entry.view.range.start,
                size: std::num::NonZeroU64::new(entry.view.range.end - entry.view.range.start),
            }),
        }));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("checked resources"),
            layout: &layout,
            entries: &entries,
        });
        let offsets = sorted
            .iter()
            .filter_map(|entry| entry.dynamic_offset)
            .collect();
        Ok(Self {
            layout,
            group,
            entries: textures.to_vec(),
            buffers: sorted,
            offsets,
        })
    }
    pub fn dynamic_offsets(&self) -> &[u32] {
        &self.offsets
    }
    pub(super) fn validate_buffers(
        &self,
        registry: &ResourceRegistry,
        uses: &[BufferUse],
    ) -> Result<(), ContractError> {
        for entry in &self.buffers {
            let expected = entry.usage()?;
            registry.check_buffer(&expected.view)?;
            if !uses.iter().any(|usage| {
                usage.view.allocation == expected.view.allocation
                    && usage.view.version == expected.view.version
                    && usage.view.range.start <= expected.view.range.start
                    && usage.view.range.end >= expected.view.range.end
                    && usage.kind == expected.kind
                    && usage.access == expected.access
            }) {
                return Err(ContractError::Binding(expected.view.allocation));
            }
        }
        Ok(())
    }
}
fn texture_layouts(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    entries: &[TextureBinding],
) -> Result<Vec<wgpu::BindGroupLayoutEntry>, ContractError> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            super::bindings::check_entry(device, registry, entry)?;
            super::bindings::check_layout(device, entries, index)?;
            Ok(wgpu::BindGroupLayoutEntry {
                binding: entry.binding,
                visibility: entry.visibility,
                ty: super::bindings::binding_type(entry, registry)?,
                count: None,
            })
        })
        .collect()
}
fn buffer_type(entry: &BufferBinding) -> wgpu::BindingType {
    let ty = match entry.kind {
        BufferBindingKind::Uniform => wgpu::BufferBindingType::Uniform,
        BufferBindingKind::Storage(access) => wgpu::BufferBindingType::Storage {
            read_only: access == AccessMode::Read,
        },
    };
    wgpu::BindingType::Buffer {
        ty,
        has_dynamic_offset: entry.dynamic_offset.is_some(),
        min_binding_size: std::num::NonZeroU64::new(entry.view.range.end - entry.view.range.start),
    }
}
fn check_buffer_binding(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    entry: &BufferBinding,
) -> Result<(), ContractError> {
    registry.check_buffer(&entry.view)?;
    let limits = device.limits();
    let (alignment, maximum, usage) = match entry.kind {
        BufferBindingKind::Uniform => (
            limits.min_uniform_buffer_offset_alignment,
            limits.max_uniform_buffer_binding_size,
            wgpu::BufferUsages::UNIFORM,
        ),
        BufferBindingKind::Storage(_) => (
            limits.min_storage_buffer_offset_alignment,
            limits.max_storage_buffer_binding_size,
            wgpu::BufferUsages::STORAGE,
        ),
    };
    let effective = entry.effective()?;
    if !entry.view.range.start.is_multiple_of(u64::from(alignment))
        || !entry.dynamic_offset.unwrap_or(0).is_multiple_of(alignment)
        || effective.range.end > entry.view.buffer.size()
    {
        return Err(ContractError::DynamicOffset(entry.view.allocation));
    }
    if entry.binding >= limits.max_bindings_per_bind_group
        || entry.visibility.is_empty()
        || wgpu::ShaderStages::from_bits(entry.visibility.bits()).is_none()
        || entry.view.range.end - entry.view.range.start > maximum
        || !entry.view.buffer.usage().contains(usage)
        || !entry.view.range.end.is_multiple_of(4)
    {
        return Err(ContractError::Binding(entry.view.allocation));
    }
    if matches!(entry.kind, BufferBindingKind::Storage(access) if access != AccessMode::Read) {
        if registry.buffer_allocation(entry.view.allocation)?.lifetime
            == super::ResourceLifetime::ImportedReadOnly
        {
            return Err(ContractError::ReadOnly(entry.view.allocation));
        }
        if entry.visibility.contains(wgpu::ShaderStages::VERTEX)
            && !device
                .features()
                .contains(wgpu::Features::VERTEX_WRITABLE_STORAGE)
        {
            return Err(ContractError::Binding(entry.view.allocation));
        }
    }
    Ok(())
}
fn check_buffer_limits(
    device: &wgpu::Device,
    entries: &[BufferBinding],
) -> Result<(), ContractError> {
    let limits = device.limits();
    for stage in [
        wgpu::ShaderStages::VERTEX,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::ShaderStages::COMPUTE,
    ] {
        let visible = entries
            .iter()
            .filter(|entry| entry.visibility.contains(stage));
        let uniforms = visible
            .clone()
            .filter(|entry| entry.kind == BufferBindingKind::Uniform)
            .count();
        let storage = visible
            .filter(|entry| matches!(entry.kind, BufferBindingKind::Storage(_)))
            .count();
        if uniforms > limits.max_uniform_buffers_per_shader_stage as usize
            || storage > limits.max_storage_buffers_per_shader_stage as usize
        {
            return Err(ContractError::BindingLayout(entries[0].view.allocation));
        }
    }
    let uniforms = entries
        .iter()
        .filter(|e| e.dynamic_offset.is_some() && e.kind == BufferBindingKind::Uniform)
        .count();
    let storage = entries
        .iter()
        .filter(|e| e.dynamic_offset.is_some() && matches!(e.kind, BufferBindingKind::Storage(_)))
        .count();
    if uniforms > limits.max_dynamic_uniform_buffers_per_pipeline_layout as usize
        || storage > limits.max_dynamic_storage_buffers_per_pipeline_layout as usize
    {
        return Err(ContractError::BindingLayout(entries[0].view.allocation));
    }
    Ok(())
}
