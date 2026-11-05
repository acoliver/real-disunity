//! Bind groups built from the resources a draw or dispatch names.

use crate::error::ProgramError;
use crate::layout::Slot;
use crate::program::GpuLayout;
use crate::resources::{BoundResource, Resource};
use crate::runtime::ProgramRuntime;

impl ProgramRuntime {
    /// One bind group for each descriptor set the program uses, with the fixed sampler at
    /// every sampler binding.
    ///
    /// # Errors
    /// Fails when a binding has no resource, a resource names no binding or the wrong kind of
    /// binding, or a buffer is smaller than the shader needs.
    pub(crate) fn bind_groups(
        &self,
        gpu: &GpuLayout,
        bound: &[BoundResource],
    ) -> Result<Vec<(u32, wgpu::BindGroup)>, ProgramError> {
        check_bound(gpu, bound)?;
        let mut groups = Vec::new();
        for (set, layout) in (0..).zip(&gpu.groups) {
            let Some(layout) = layout else { continue };
            let mut entries = Vec::new();
            for slot in gpu.layout.slots().iter().filter(|s| s.set == set) {
                entries.push(wgpu::BindGroupEntry {
                    binding: slot.binding,
                    resource: self.slot_resource(slot, bound)?,
                });
            }
            let group = self.guarded(|| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("render-program bind group"),
                    layout,
                    entries: &entries,
                })
            })?;
            groups.push((set, group));
        }
        Ok(groups)
    }

    fn slot_resource<'a>(
        &'a self,
        slot: &Slot,
        bound: &[BoundResource],
    ) -> Result<wgpu::BindingResource<'a>, ProgramError> {
        if matches!(slot.ty, wgpu::BindingType::Sampler(_)) {
            return Ok(wgpu::BindingResource::Sampler(&self.sampler));
        }
        let missing = ProgramError::MissingBinding {
            set: slot.set,
            binding: slot.binding,
        };
        let item = bound
            .iter()
            .find(|b| (b.set, b.binding) == (slot.set, slot.binding))
            .ok_or(missing)?;
        match (&slot.ty, item.resource) {
            (wgpu::BindingType::Buffer { .. }, Resource::Buffer(handle)) => {
                let entry = self.store.buffer(handle)?;
                if entry.len < slot.min_size {
                    return Err(ProgramError::BufferTooSmall {
                        set: slot.set,
                        binding: slot.binding,
                        needed: slot.min_size,
                        have: entry.len,
                    });
                }
                Ok(entry.buffer.as_entire_binding())
            }
            (wgpu::BindingType::Texture { .. }, Resource::Texture(handle)) => {
                let entry = self.store.texture(handle)?;
                check_texture(slot, &entry.texture)?;
                Ok(wgpu::BindingResource::TextureView(&entry.view))
            }
            (expected, found) => Err(ProgramError::WrongResource {
                set: slot.set,
                binding: slot.binding,
                expected: format!("{expected:?}"),
                found: format!("{found:?}"),
            }),
        }
    }
}

/// Every bound resource names a distinct, non-sampler binding of the program.
fn check_bound(gpu: &GpuLayout, bound: &[BoundResource]) -> Result<(), ProgramError> {
    for (index, item) in bound.iter().enumerate() {
        let known = gpu
            .layout
            .slot(item.set, item.binding)
            .is_some_and(|slot| !matches!(slot.ty, wgpu::BindingType::Sampler(_)));
        let repeated = bound[..index]
            .iter()
            .any(|other| (other.set, other.binding) == (item.set, item.binding));
        if !known || repeated {
            return Err(ProgramError::UnexpectedBinding {
                set: item.set,
                binding: item.binding,
            });
        }
    }
    Ok(())
}

/// A bound texture is a single-sample 2D texture whose format suits the shader's sample type.
fn check_texture(slot: &Slot, texture: &wgpu::Texture) -> Result<(), ProgramError> {
    let wgpu::BindingType::Texture {
        sample_type,
        view_dimension,
        multisampled,
    } = slot.ty
    else {
        return Ok(());
    };
    let format_type = texture.format().sample_type(None, None);
    let suits = match (sample_type, format_type) {
        (
            wgpu::TextureSampleType::Float { filterable },
            Some(wgpu::TextureSampleType::Float {
                filterable: format_filterable,
            }),
        ) => format_filterable || !filterable,
        (wanted, Some(found)) => wanted == found,
        (_, None) => false,
    };
    let is_2d = view_dimension == wgpu::TextureViewDimension::D2 && !multisampled;
    if suits && is_2d {
        return Ok(());
    }
    Err(ProgramError::WrongResource {
        set: slot.set,
        binding: slot.binding,
        expected: format!("{sample_type:?} {view_dimension:?} multisampled={multisampled}"),
        found: format!("{:?} 2D texture", texture.format()),
    })
}
