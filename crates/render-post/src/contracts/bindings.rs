use super::{
    AccessMode, CheckedView, ContractError, RasterPass, ResourceLifetime, ResourceRegistry,
    Semantic, UseKind,
};

#[derive(Clone, Copy, Debug)]
pub enum TextureBindingKind {
    Sampled(wgpu::TextureSampleType),
    Storage(wgpu::StorageTextureAccess),
}
#[derive(Clone, Debug)]
pub struct TextureBinding {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub view: CheckedView,
    pub samples: u32,
    pub semantic: Semantic,
    pub kind: TextureBindingKind,
}

/// Owns the actual layout, bind group and entries used to create them.
#[derive(Clone, Debug)]
pub struct CheckedBindings {
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    entries: Vec<TextureBinding>,
}
impl CheckedBindings {
    pub fn build(
        device: &wgpu::Device,
        registry: &ResourceRegistry,
        entries: &[TextureBinding],
    ) -> Result<Self, ContractError> {
        let mut layout_entries = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            check_entry(device, registry, entry)?;
            check_layout(device, entries, index)?;
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: entry.binding,
                visibility: entry.visibility,
                ty: binding_type(entry, registry)?,
                count: None,
            });
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("checked texture layout"),
            entries: &layout_entries,
        });
        let gpu_entries: Vec<_> = entries
            .iter()
            .map(|entry| wgpu::BindGroupEntry {
                binding: entry.binding,
                resource: wgpu::BindingResource::TextureView(&entry.view.view),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("checked texture bindings"),
            layout: &layout,
            entries: &gpu_entries,
        });
        Ok(Self {
            layout,
            group,
            entries: entries.to_vec(),
        })
    }
    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }
    pub fn group(&self) -> &wgpu::BindGroup {
        &self.group
    }
    pub fn validate(
        &self,
        registry: &ResourceRegistry,
        pass: &RasterPass,
    ) -> Result<(), ContractError> {
        for entry in &self.entries {
            registry.check_view(&entry.view)?;
            let (kind, access) = match entry.kind {
                TextureBindingKind::Sampled(_) => (UseKind::Sampled, AccessMode::Read),
                TextureBindingKind::Storage(wgpu::StorageTextureAccess::ReadOnly) => {
                    (UseKind::Storage, AccessMode::Read)
                }
                TextureBindingKind::Storage(wgpu::StorageTextureAccess::WriteOnly) => {
                    (UseKind::Storage, AccessMode::Write)
                }
                TextureBindingKind::Storage(_) => (UseKind::Storage, AccessMode::ReadWrite),
            };
            if !pass.uses.iter().any(|usage| {
                usage.view.allocation == entry.view.allocation
                    && usage.view.version == entry.view.version
                    && usage.view.range.contains(&entry.view.range)
                    && usage.kind == kind
                    && usage.access == access
            }) {
                return Err(ContractError::Binding(entry.view.allocation));
            }
        }
        Ok(())
    }
}

fn check_layout(
    device: &wgpu::Device,
    entries: &[TextureBinding],
    index: usize,
) -> Result<(), ContractError> {
    let entry = &entries[index];
    let limits = device.limits();
    if entry.binding >= limits.max_bindings_per_bind_group
        || wgpu::ShaderStages::from_bits(entry.visibility.bits()).is_none()
        || entries[..index]
            .iter()
            .any(|other| other.binding == entry.binding)
    {
        return Err(ContractError::BindingLayout(entry.view.allocation));
    }
    for stage in [
        wgpu::ShaderStages::VERTEX,
        wgpu::ShaderStages::FRAGMENT,
        wgpu::ShaderStages::COMPUTE,
    ] {
        let visible = entries
            .iter()
            .filter(|entry| entry.visibility.contains(stage));
        let sampled = visible
            .clone()
            .filter(|entry| matches!(entry.kind, TextureBindingKind::Sampled(_)))
            .count();
        let storage = visible
            .filter(|entry| matches!(entry.kind, TextureBindingKind::Storage(_)))
            .count();
        if sampled > limits.max_sampled_textures_per_shader_stage as usize
            || storage > limits.max_storage_textures_per_shader_stage as usize
        {
            return Err(ContractError::BindingLayout(entry.view.allocation));
        }
    }
    Ok(())
}

fn check_entry(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    entry: &TextureBinding,
) -> Result<(), ContractError> {
    registry.check_view(&entry.view)?;
    let allocation = registry.allocation(entry.view.allocation)?;
    let texture = &allocation.texture;
    let usage = match entry.kind {
        TextureBindingKind::Sampled(sample_type) => {
            check_sample_type(device, entry, texture, sample_type)?;
            wgpu::TextureUsages::TEXTURE_BINDING
        }
        TextureBindingKind::Storage(access) => {
            check_storage(device, entry, texture, access)?;
            if access != wgpu::StorageTextureAccess::ReadOnly
                && allocation.lifetime == ResourceLifetime::ImportedReadOnly
            {
                return Err(ContractError::ReadOnly(allocation.key));
            }
            wgpu::TextureUsages::STORAGE_BINDING
        }
    };
    if entry.samples != texture.sample_count()
        || entry.semantic != allocation.semantic
        || !texture.usage().contains(usage)
    {
        return Err(ContractError::Binding(allocation.key));
    }
    Ok(())
}

fn check_sample_type(
    device: &wgpu::Device,
    entry: &TextureBinding,
    texture: &wgpu::Texture,
    sample_type: wgpu::TextureSampleType,
) -> Result<(), ContractError> {
    use wgpu::TextureSampleType as Type;
    let actual = texture
        .format()
        .sample_type(Some(entry.view.range.aspect), Some(device.features()));
    let compatible = actual == Some(sample_type)
        || matches!(
            (actual, sample_type),
            (
                Some(Type::Float { .. } | Type::Depth),
                Type::Float { filterable: false }
            )
        );
    if !compatible
        || (entry.samples > 1
            && (entry.view.range.layers.len() != 1
                || sample_type == Type::Float { filterable: true }))
    {
        return Err(ContractError::SampleType(entry.view.allocation));
    }
    Ok(())
}

fn check_storage(
    device: &wgpu::Device,
    entry: &TextureBinding,
    texture: &wgpu::Texture,
    access: wgpu::StorageTextureAccess,
) -> Result<(), ContractError> {
    use wgpu::{StorageTextureAccess as Access, TextureFormatFeatureFlags as Flags};
    let required = match access {
        Access::ReadOnly => Flags::STORAGE_READ_ONLY,
        Access::WriteOnly => Flags::STORAGE_WRITE_ONLY,
        Access::ReadWrite => Flags::STORAGE_READ_WRITE,
        Access::Atomic => Flags::STORAGE_ATOMIC,
    };
    let features = device.features();
    if entry.samples != 1
        || entry.view.range.mips.len() != 1
        || !texture
            .format()
            .guaranteed_format_features(features)
            .flags
            .contains(required)
        || (access == Access::Atomic
            && (!features.contains(wgpu::Features::TEXTURE_ATOMIC)
                || !texture
                    .usage()
                    .contains(wgpu::TextureUsages::STORAGE_ATOMIC)))
        || (access != Access::ReadOnly
            && entry.visibility.contains(wgpu::ShaderStages::VERTEX)
            && !features.contains(wgpu::Features::VERTEX_WRITABLE_STORAGE))
    {
        return Err(ContractError::StorageAccess {
            allocation: entry.view.allocation,
            format: texture.format(),
            access,
        });
    }
    Ok(())
}

fn binding_type(
    entry: &TextureBinding,
    registry: &ResourceRegistry,
) -> Result<wgpu::BindingType, ContractError> {
    let view_dimension = if entry.view.range.layers.len() == 1 {
        wgpu::TextureViewDimension::D2
    } else {
        wgpu::TextureViewDimension::D2Array
    };
    Ok(match entry.kind {
        TextureBindingKind::Sampled(sample_type) => wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: entry.samples > 1,
        },
        TextureBindingKind::Storage(access) => wgpu::BindingType::StorageTexture {
            access,
            format: registry.allocation(entry.view.allocation)?.texture.format(),
            view_dimension,
        },
    })
}
