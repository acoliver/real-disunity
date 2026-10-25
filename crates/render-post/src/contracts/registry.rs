use std::sync::atomic::{AtomicU64, Ordering};

use super::{AllocationKey, CheckedView, ContractError, ResourceLifetime, Semantic, TextureRange};

static NEXT_ALLOCATION: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_key() -> AllocationKey {
    AllocationKey {
        id: NEXT_ALLOCATION.fetch_add(1, Ordering::Relaxed),
        generation: 0,
    }
}

pub(super) struct Allocation {
    pub key: AllocationKey,
    pub texture: wgpu::Texture,
    pub lifetime: ResourceLifetime,
    pub semantic: Semantic,
    pub initial: Option<u32>,
    pub view_formats: Vec<wgpu::TextureFormat>,
    pub lease: Option<worldrunner_render_gpu::submission::FrameLease>,
}

/// Physical identity is shared across cloned imports. Replacements invalidate old views.
#[derive(Default)]
pub struct ResourceRegistry {
    pub(super) allocations: Vec<Allocation>,
    pub(super) buffers: Vec<super::buffers::BufferAllocation>,
    pub(super) retired_buffers: Vec<(AllocationKey, wgpu::Buffer)>,
    retired: Vec<(AllocationKey, wgpu::Texture)>,
}
impl ResourceRegistry {
    pub fn import(
        &mut self,
        texture: &wgpu::Texture,
        lifetime: ResourceLifetime,
        semantic: Semantic,
        initial: Option<u32>,
    ) -> Result<AllocationKey, ContractError> {
        self.import_with_view_formats(texture, lifetime, semantic, initial, &[])
    }
    pub(super) fn import_with_view_formats(
        &mut self,
        texture: &wgpu::Texture,
        lifetime: ResourceLifetime,
        semantic: Semantic,
        initial: Option<u32>,
        view_formats: &[wgpu::TextureFormat],
    ) -> Result<AllocationKey, ContractError> {
        if let Some((key, _)) = self.retired.iter().find(|(_, retired)| retired == texture) {
            return Err(ContractError::StaleAllocation(*key));
        }
        if let Some(existing) = self
            .allocations
            .iter()
            .find(|entry| entry.texture == *texture)
        {
            if (existing.lifetime, existing.semantic, existing.initial)
                != (lifetime, semantic, initial)
                || existing.view_formats != view_formats
            {
                return Err(ContractError::Identity(existing.key));
            }
            if let Some(lease) = &existing.lease {
                lease.check().map_err(ContractError::Lease)?;
            }
            return Ok(existing.key);
        }
        let key = next_key();
        self.allocations.push(Allocation {
            key,
            texture: texture.clone(),
            lifetime,
            semantic,
            initial,
            view_formats: view_formats.to_vec(),
            lease: None,
        });
        Ok(key)
    }
    /// Imports a texture owned by an external frame capability.
    pub fn import_leased(
        &mut self,
        texture: &wgpu::Texture,
        lease: &worldrunner_render_gpu::submission::FrameLease,
        semantic: Semantic,
        initial: Option<u32>,
    ) -> Result<AllocationKey, ContractError> {
        lease.check().map_err(ContractError::Lease)?;
        if let Some(entry) = self
            .allocations
            .iter()
            .find(|entry| entry.texture == *texture)
            && !entry
                .lease
                .as_ref()
                .is_some_and(|old| old.same_owner(lease))
        {
            return Err(ContractError::Identity(entry.key));
        }
        let key = self.import(texture, ResourceLifetime::FrameLocal, semantic, initial)?;
        self.allocations
            .iter_mut()
            .find(|entry| entry.key == key)
            .unwrap()
            .lease = Some(lease.clone());
        Ok(key)
    }
    /// New backing storage has no declared initialized contents. Read-only imports
    /// cannot be replaced; import their new backing with an explicit initial version.
    pub fn replace(
        &mut self,
        key: AllocationKey,
        texture: &wgpu::Texture,
    ) -> Result<AllocationKey, ContractError> {
        if self.allocation(key)?.lease.is_some() {
            return Err(ContractError::Identity(key));
        }
        if self.allocation(key)?.lifetime == ResourceLifetime::ImportedReadOnly {
            return Err(ContractError::ReadOnlyReplacement(key));
        }
        if self
            .allocations
            .iter()
            .any(|entry| entry.texture == *texture)
            || self.retired.iter().any(|(_, retired)| retired == texture)
        {
            return Err(ContractError::Identity(key));
        }
        let entry = self
            .allocations
            .iter_mut()
            .find(|entry| entry.key == key)
            .unwrap();
        entry.key.generation = key
            .generation
            .checked_add(1)
            .ok_or(ContractError::Identity(key))?;
        self.retired.push((key, entry.texture.clone()));
        entry.texture = texture.clone();
        entry.initial = None;
        entry.view_formats.clear();
        Ok(entry.key)
    }
    pub(super) fn allocation(&self, key: AllocationKey) -> Result<&Allocation, ContractError> {
        let entry = self
            .allocations
            .iter()
            .find(|entry| entry.key == key)
            .ok_or(ContractError::StaleAllocation(key))?;
        if let Some(lease) = &entry.lease {
            lease.check().map_err(ContractError::Lease)?;
        }
        Ok(entry)
    }
    pub fn check_view(&self, view: &CheckedView) -> Result<(), ContractError> {
        let entry = self.allocation(view.allocation)?;
        if let Some(lease) = &view.lease {
            lease.check().map_err(ContractError::Lease)?;
            if !entry
                .lease
                .as_ref()
                .is_some_and(|owner| owner.same_owner(lease))
            {
                return Err(ContractError::Identity(view.allocation));
            }
        }
        if entry.texture != *view.view.texture() {
            return Err(ContractError::Identity(view.allocation));
        }
        validate_range(entry, &view.range)
    }
    pub fn view(
        &self,
        key: AllocationKey,
        range: TextureRange,
        version: u32,
    ) -> Result<CheckedView, ContractError> {
        let entry = self.allocation(key)?;
        validate_range(entry, &range)?;
        let dimension = if range.layers.end - range.layers.start == 1 {
            wgpu::TextureViewDimension::D2
        } else {
            wgpu::TextureViewDimension::D2Array
        };
        self.view_dimension(key, range, version, dimension)
    }
    pub fn view_dimension(
        &self,
        key: AllocationKey,
        range: TextureRange,
        version: u32,
        dimension: wgpu::TextureViewDimension,
    ) -> Result<CheckedView, ContractError> {
        let format = self.allocation(key)?.texture.format();
        self.view_format(key, range, version, dimension, format)
    }
    /// Selects the allocation format or one of its creation-time view formats.
    ///
    /// For example, an anti-aliasing pass can target `Rgba8UnormSrgb` while the
    /// texture uses `Rgba8Unorm` storage. Import its creation-time declaration
    /// with [`Self::create`] or [`super::DeclaredTexture`] before selecting the sRGB view.
    /// [`Self::view`] and [`Self::view_dimension`] keep the allocation format.
    pub fn view_format(
        &self,
        key: AllocationKey,
        range: TextureRange,
        version: u32,
        dimension: wgpu::TextureViewDimension,
        format: wgpu::TextureFormat,
    ) -> Result<CheckedView, ContractError> {
        let entry = self.allocation(key)?;
        validate_range(entry, &range)?;
        let layers = range.layers.end - range.layers.start;
        let width = entry.texture.width();
        let height = entry.texture.height();
        let valid = match dimension {
            wgpu::TextureViewDimension::D2 => layers == 1,
            wgpu::TextureViewDimension::D2Array => true,
            wgpu::TextureViewDimension::Cube => {
                layers == 6 && width == height && entry.texture.sample_count() == 1
            }
            wgpu::TextureViewDimension::CubeArray => {
                layers.is_multiple_of(6) && width == height && entry.texture.sample_count() == 1
            }
            _ => false,
        };
        if !valid
            || (entry.texture.sample_count() > 1
                && (dimension != wgpu::TextureViewDimension::D2 || range.mips.len() != 1))
        {
            return Err(ContractError::Range(key));
        }
        if format != entry.texture.format() && !entry.view_formats.contains(&format) {
            return Err(ContractError::ViewFormat {
                allocation: key,
                format,
            });
        }
        let usage = super::declared_texture::view_usage(&entry.texture, format);
        let view = entry.texture.create_view(&wgpu::TextureViewDescriptor {
            format: (format != entry.texture.format()).then_some(format),
            usage: Some(usage),
            dimension: Some(dimension),
            aspect: range.aspect,
            base_mip_level: range.mips.start,
            mip_level_count: Some(range.mips.end - range.mips.start),
            base_array_layer: range.layers.start,
            array_layer_count: Some(range.layers.end - range.layers.start),
            ..Default::default()
        });
        Ok(CheckedView {
            allocation: key,
            range,
            version,
            view,
            dimension,
            format,
            usage,
            lease: entry.lease.clone(),
        })
    }
}

fn validate_range(entry: &Allocation, range: &TextureRange) -> Result<(), ContractError> {
    let texture = &entry.texture;
    let format = texture.format();
    let aspect_valid = match range.aspect {
        wgpu::TextureAspect::All => true,
        wgpu::TextureAspect::DepthOnly => format.has_depth_aspect(),
        wgpu::TextureAspect::StencilOnly => format.has_stencil_aspect(),
        _ => false,
    };
    if texture.dimension() != wgpu::TextureDimension::D2
        || !aspect_valid
        || range.mips.start >= range.mips.end
        || range.mips.end > texture.mip_level_count()
        || range.layers.start >= range.layers.end
        || range.layers.end > texture.depth_or_array_layers()
    {
        return Err(ContractError::Range(entry.key));
    }
    Ok(())
}
