//! Bind groups built from the resources a draw or dispatch names.

use std::sync::Arc;

use crate::binding::{BindingAddress, BindingSource};
use crate::budget::Reservation;
use crate::error::ProgramError;
use crate::import::{Import, ImportedResource};
use crate::layout::Slot;
use crate::program::GpuLayout;
use crate::resources::{BoundResource, Resource};
use crate::runtime::ProgramRuntime;

/// A constant buffer that belongs to one prepared draw rather than to a handle: the bytes a
/// material's properties packed when the draw was prepared.
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub(crate) at: BindingAddress,
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) len: u64,
    pub(crate) reservation: Arc<Reservation>,
}

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
        self.bind_groups_with(gpu, bound, &[])
    }

    /// Like [`bind_groups`](Self::bind_groups), with `snapshots` filling the buffer bindings
    /// they are at.
    pub(crate) fn bind_groups_with(
        &self,
        gpu: &GpuLayout,
        bound: &[BoundResource],
        snapshots: &[Snapshot],
    ) -> Result<Vec<(u32, wgpu::BindGroup)>, ProgramError> {
        self.bind_groups_importing(gpu, bound, snapshots, &[])
    }

    /// Like [`bind_groups_with`](Self::bind_groups_with), with `imports` filling the bindings
    /// the program sources from an engine semantic.
    ///
    /// # Errors
    /// Also fails when a binding that takes an import has none, when an import is given twice
    /// or names no binding, and when an import is not the kind of resource its binding is.
    pub(crate) fn bind_groups_importing(
        &self,
        gpu: &GpuLayout,
        bound: &[BoundResource],
        snapshots: &[Snapshot],
        imports: &[Import],
    ) -> Result<Vec<(u32, wgpu::BindGroup)>, ProgramError> {
        check_bound(gpu, bound, snapshots)?;
        check_imports(gpu, imports)?;
        let mut groups = Vec::new();
        for (set, layout) in (0..).zip(&gpu.groups) {
            let Some(layout) = layout else { continue };
            if gpu.is_external(set) {
                continue;
            }
            let mut entries = Vec::new();
            for slot in gpu.layout.slots().iter().filter(|s| s.set == set) {
                entries.push(wgpu::BindGroupEntry {
                    binding: slot.binding,
                    resource: self.slot_resource(slot, (bound, snapshots, imports))?,
                });
            }
            let group = self.guarded(|| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("render-program bind group"),
                    layout,
                    entries: &entries,
                })
            })?;
            self.counters.bind_group_built();
            groups.push((set, group));
        }
        Ok(groups)
    }

    fn slot_resource<'a>(
        &'a self,
        slot: &Slot,
        (bound, snapshots, imports): (&[BoundResource], &'a [Snapshot], &'a [Import]),
    ) -> Result<wgpu::BindingResource<'a>, ProgramError> {
        if let BindingSource::Engine(semantic) = &slot.source {
            let import = imports
                .iter()
                .find(|import| &import.semantic == semantic)
                .ok_or_else(|| ProgramError::MissingImport {
                    semantic: semantic.to_string(),
                    set: slot.set,
                    binding: slot.binding,
                })?;
            return imported_resource(slot, &import.resource);
        }
        if matches!(slot.ty, wgpu::BindingType::Sampler(_)) {
            return Ok(wgpu::BindingResource::Sampler(&self.sampler));
        }
        let address = BindingAddress {
            set: slot.set,
            binding: slot.binding,
        };
        if let Some(snapshot) = snapshots.iter().find(|snapshot| snapshot.at == address) {
            return snapshot_resource(slot, snapshot);
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

/// The imported resource as the binding's entry, when it is the kind of resource the binding is.
fn imported_resource<'a>(
    slot: &Slot,
    resource: &'a ImportedResource,
) -> Result<wgpu::BindingResource<'a>, ProgramError> {
    match (&slot.ty, resource) {
        (wgpu::BindingType::Buffer { .. }, ImportedResource::Buffer(buffer)) => {
            if buffer.size() < slot.min_size {
                return Err(ProgramError::BufferTooSmall {
                    set: slot.set,
                    binding: slot.binding,
                    needed: slot.min_size,
                    have: buffer.size(),
                });
            }
            Ok(buffer.as_entire_binding())
        }
        (wgpu::BindingType::Texture { .. }, ImportedResource::Texture(texture)) => {
            check_texture(slot, &texture.texture)?;
            Ok(wgpu::BindingResource::TextureView(&texture.view))
        }
        (wgpu::BindingType::Sampler(_), ImportedResource::Sampler(sampler)) => {
            Ok(wgpu::BindingResource::Sampler(sampler))
        }
        (expected, found) => Err(ProgramError::WrongResource {
            set: slot.set,
            binding: slot.binding,
            expected: format!("{expected:?}"),
            found: found.describe().to_owned(),
        }),
    }
}

/// Every import is given once and is taken by a binding of the program.
fn check_imports(gpu: &GpuLayout, imports: &[Import]) -> Result<(), ProgramError> {
    for (index, import) in imports.iter().enumerate() {
        if imports[..index]
            .iter()
            .any(|other| other.semantic == import.semantic)
        {
            return Err(ProgramError::DuplicateImport {
                semantic: import.semantic.to_string(),
            });
        }
        let semantic = BindingSource::Engine(import.semantic.clone());
        if let Some(lent) = gpu
            .external
            .iter()
            .find(|lent| lent.bindings.bindings.iter().any(|b| b.source == semantic))
        {
            return Err(ProgramError::ExternalSetConflict { set: lent.set });
        }
        let taken = gpu
            .layout
            .slots()
            .iter()
            .any(|slot| slot.source == semantic);
        if !taken {
            return Err(ProgramError::UnusedImport {
                semantic: import.semantic.to_string(),
            });
        }
    }
    Ok(())
}

fn snapshot_resource<'a>(
    slot: &Slot,
    snapshot: &'a Snapshot,
) -> Result<wgpu::BindingResource<'a>, ProgramError> {
    if !matches!(slot.ty, wgpu::BindingType::Buffer { .. }) {
        return Err(ProgramError::WrongResource {
            set: slot.set,
            binding: slot.binding,
            expected: format!("{:?}", slot.ty),
            found: "a constant buffer".to_owned(),
        });
    }
    if snapshot.len < slot.min_size {
        return Err(ProgramError::BufferTooSmall {
            set: slot.set,
            binding: slot.binding,
            needed: slot.min_size,
            have: snapshot.len,
        });
    }
    Ok(snapshot.buffer.as_entire_binding())
}

/// Every bound resource names a distinct binding of the program that is neither a sampler, nor
/// sourced from an engine semantic, nor filled by a snapshot.
fn check_bound(
    gpu: &GpuLayout,
    bound: &[BoundResource],
    snapshots: &[Snapshot],
) -> Result<(), ProgramError> {
    for (index, item) in bound.iter().enumerate() {
        if gpu.is_external(item.set) {
            return Err(ProgramError::ExternalSetConflict { set: item.set });
        }
        let known = gpu.layout.slot(item.set, item.binding).is_some_and(|slot| {
            !matches!(slot.ty, wgpu::BindingType::Sampler(_))
                && !matches!(slot.source, BindingSource::Engine(_))
        });
        let repeated = bound[..index]
            .iter()
            .any(|other| (other.set, other.binding) == (item.set, item.binding));
        let filled = snapshots
            .iter()
            .any(|snapshot| snapshot.at.set == item.set && snapshot.at.binding == item.binding);
        if !known || repeated || filled {
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
