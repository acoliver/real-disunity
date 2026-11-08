//! Bind groups built from the resources a draw or dispatch names.

use std::sync::Arc;

use crate::binding::{BindingAddress, BindingSource, EngineSemantic};
use crate::budget::Reservation;
use crate::error::{ProgramError, StorageImageError};
use crate::import::{Import, ImportedResource, default_dimension};
use crate::layout::Slot;
use crate::program::{ComputeProgram, GpuLayout};
use crate::resources::{BoundResource, Resource};
use crate::runtime::ProgramRuntime;
use crate::set::{ImportTable, PreparedSet};

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
        self.bind_groups_sharing(gpu, (bound, snapshots, imports), &[])
    }

    /// Like [`bind_groups_importing`](Self::bind_groups_importing), with the groups of `prepared`
    /// used for their sets instead of building one.
    ///
    /// # Errors
    /// Also fails when a prepared set is given twice, is for a set the pass owner binds, does not
    /// fit the program's layout at its set, or when the draw also gives that set resources.
    pub(crate) fn bind_groups_sharing(
        &self,
        gpu: &GpuLayout,
        (bound, snapshots, imports): (&[BoundResource], &[Snapshot], &[Import]),
        prepared: &[&PreparedSet],
    ) -> Result<Vec<(u32, wgpu::BindGroup)>, ProgramError> {
        check_prepared(gpu, prepared, self.registry.runtime())?;
        check_bound(gpu, bound, snapshots, prepared)?;
        check_imports(gpu, imports, prepared)?;
        self.check_aliasing(gpu, bound, imports)?;
        let sources = Sources {
            bound,
            snapshots,
            imports: Imports::List(imports),
        };
        let mut groups = Vec::new();
        for (set, layout) in (0..).zip(&gpu.groups) {
            let Some(layout) = layout else { continue };
            if gpu.is_external(set) {
                continue;
            }
            if let Some(ready) = prepared.iter().find(|ready| ready.set == set) {
                groups.push((set, ready.group.clone()));
                continue;
            }
            groups.push((set, self.build_group(gpu, (set, layout), sources)?));
        }
        Ok(groups)
    }

    /// The bind group for `set`, built from `sources`.
    pub(crate) fn build_group(
        &self,
        gpu: &GpuLayout,
        (set, layout): (u32, &wgpu::BindGroupLayout),
        sources: Sources<'_>,
    ) -> Result<wgpu::BindGroup, ProgramError> {
        let mut resolved = Vec::new();
        for slot in gpu.layout.slots().iter().filter(|s| s.set == set) {
            resolved.push((slot.binding, self.slot_resource(slot, sources)?));
        }
        let entries: Vec<_> = resolved
            .iter()
            .map(|(binding, resource)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: resource.entry(),
            })
            .collect();
        let group = self.guarded(|| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render-program bind group"),
                layout,
                entries: &entries,
            })
        })?;
        self.counters.bind_group_built();
        Ok(group)
    }

    fn slot_resource<'a>(
        &'a self,
        slot: &Slot,
        Sources {
            bound,
            snapshots,
            imports,
        }: Sources<'a>,
    ) -> Result<Resolved<'a>, ProgramError> {
        if let BindingSource::Engine(semantic) = &slot.source {
            let import = imports
                .find(semantic)
                .ok_or_else(|| ProgramError::MissingImport {
                    semantic: semantic.to_string(),
                    set: slot.set,
                    binding: slot.binding,
                })?;
            return imported_resource(slot, import).map(Resolved::Borrowed);
        }
        if matches!(slot.ty, wgpu::BindingType::Sampler(_)) {
            return Ok(Resolved::Borrowed(wgpu::BindingResource::Sampler(
                &self.sampler,
            )));
        }
        let address = BindingAddress {
            set: slot.set,
            binding: slot.binding,
        };
        if let Some(snapshot) = snapshots.iter().find(|snapshot| snapshot.at == address) {
            return snapshot_resource(slot, snapshot).map(Resolved::Borrowed);
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
                Ok(Resolved::Borrowed(entry.buffer.as_entire_binding()))
            }
            (wgpu::BindingType::Texture { .. }, Resource::Texture(handle)) => {
                let entry = self.store.texture(handle)?;
                check_texture(slot, &entry.texture, default_dimension(&entry.texture))?;
                Ok(Resolved::Borrowed(wgpu::BindingResource::TextureView(
                    &entry.view,
                )))
            }
            (
                wgpu::BindingType::StorageTexture { view_dimension, .. },
                Resource::Texture(handle),
            ) => {
                let entry = self.store.texture(handle)?;
                check_storage_image(slot, &entry.texture, None)?;
                // A storage binding takes one mip level, so the view is made for the slot.
                let view = self.guarded(|| {
                    entry.texture.create_view(&wgpu::TextureViewDescriptor {
                        label: Some("render-program storage view"),
                        dimension: Some(*view_dimension),
                        base_mip_level: 0,
                        mip_level_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    })
                })?;
                Ok(Resolved::Owned(view))
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

/// What a slot's bind group entry holds: a resource that outlives the group's construction, or a
/// view made for the slot, which does not.
enum Resolved<'a> {
    Borrowed(wgpu::BindingResource<'a>),
    Owned(wgpu::TextureView),
}

impl Resolved<'_> {
    fn entry(&self) -> wgpu::BindingResource<'_> {
        match self {
            Self::Borrowed(resource) => resource.clone(),
            Self::Owned(view) => wgpu::BindingResource::TextureView(view),
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
            check_texture(slot, &texture.texture, texture.dimension)?;
            Ok(wgpu::BindingResource::TextureView(&texture.view))
        }
        (wgpu::BindingType::StorageTexture { .. }, ImportedResource::Texture(texture)) => {
            check_storage_image(slot, &texture.texture, Some(texture.dimension))?;
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

/// Everything a bind group's slots are filled from.
#[derive(Clone, Copy)]
pub(crate) struct Sources<'a> {
    pub(crate) bound: &'a [BoundResource],
    pub(crate) snapshots: &'a [Snapshot],
    pub(crate) imports: Imports<'a>,
}

/// Where the imports of an engine semantic are looked up.
#[derive(Clone, Copy)]
pub(crate) enum Imports<'a> {
    /// A draw's list, which must hold each import once.
    List(&'a [Import]),
    /// A table, which may hold entries nothing takes.
    Table(&'a ImportTable),
}

impl<'a> Imports<'a> {
    fn find(self, semantic: &EngineSemantic) -> Option<&'a ImportedResource> {
        match self {
            Self::List(imports) => imports
                .iter()
                .find(|import| &import.semantic == semantic)
                .map(|import| &import.resource),
            Self::Table(table) => table.get(semantic),
        }
    }
}

/// The prepared sets are given once each, for sets the runtime builds, with the layout the
/// program has there, by the runtime that holds that layout.
fn check_prepared(
    gpu: &GpuLayout,
    prepared: &[&PreparedSet],
    runtime: u64,
) -> Result<(), ProgramError> {
    for (index, ready) in prepared.iter().enumerate() {
        let set = ready.set;
        if prepared[..index].iter().any(|other| other.set == set) {
            return Err(ProgramError::PreparedSetConflict { set });
        }
        if gpu.is_external(set) {
            return Err(ProgramError::ExternalSetConflict { set });
        }
        let key = usize::try_from(set)
            .ok()
            .and_then(|set| gpu.set_keys.get(set))
            .copied()
            .flatten();
        if ready.runtime != runtime || key != Some(ready.key) {
            return Err(ProgramError::SetLayoutMismatch { set });
        }
    }
    Ok(())
}

/// Every import is given once and is taken by a binding of the program that no prepared set
/// fills.
fn check_imports(
    gpu: &GpuLayout,
    imports: &[Import],
    prepared: &[&PreparedSet],
) -> Result<(), ProgramError> {
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
        let Some(slot) = gpu
            .layout
            .slots()
            .iter()
            .find(|slot| slot.source == semantic)
        else {
            return Err(ProgramError::UnusedImport {
                semantic: import.semantic.to_string(),
            });
        };
        if prepared.iter().any(|ready| ready.set == slot.set) {
            return Err(ProgramError::PreparedSetConflict { set: slot.set });
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
/// sourced from an engine semantic, nor filled by a snapshot, and is not in a prepared set.
fn check_bound(
    gpu: &GpuLayout,
    bound: &[BoundResource],
    snapshots: &[Snapshot],
    prepared: &[&PreparedSet],
) -> Result<(), ProgramError> {
    for (index, item) in bound.iter().enumerate() {
        if gpu.is_external(item.set) {
            return Err(ProgramError::ExternalSetConflict { set: item.set });
        }
        if prepared.iter().any(|ready| ready.set == item.set) {
            return Err(ProgramError::PreparedSetConflict { set: item.set });
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

/// A bound texture suits the slot when its view has the slot's dimension, its sample count
/// agrees with the slot's multisampling and its format suits the shader's sample type.
fn check_texture(
    slot: &Slot,
    texture: &wgpu::Texture,
    dimension: wgpu::TextureViewDimension,
) -> Result<(), ProgramError> {
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
    let samples = texture.sample_count();
    if suits && view_dimension == dimension && multisampled == (samples > 1) {
        return Ok(());
    }
    Err(ProgramError::WrongResource {
        set: slot.set,
        binding: slot.binding,
        expected: format!("{sample_type:?} {view_dimension:?} multisampled={multisampled}"),
        found: format!(
            "{:?} {dimension:?} texture with {samples} samples",
            texture.format()
        ),
    })
}

/// Whether the slot lets the shader write the texture.
fn writes(slot: &Slot) -> bool {
    matches!(
        slot.ty,
        wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly
                | wgpu::StorageTextureAccess::ReadWrite
                | wgpu::StorageTextureAccess::Atomic,
            ..
        }
    )
}

impl ProgramRuntime {
    /// Checks the storage images of a dispatch or draw without recording anything: every texture
    /// bound to a storage image slot suits it, and no texture a pass writes is bound twice.
    ///
    /// # Errors
    /// Fails with `StorageImage` for a texture that does not suit its slot, with
    /// `AliasedTexture` for a texture bound to two slots of which one writes it, and on a handle
    /// the runtime does not hold.
    pub fn check_storage_images(
        &self,
        program: &ComputeProgram,
        bound: &[BoundResource],
    ) -> Result<(), ProgramError> {
        let gpu = &program.gpu;
        for item in bound {
            let Some(slot) = gpu.layout.slot(item.set, item.binding) else {
                continue;
            };
            if let (wgpu::BindingType::StorageTexture { .. }, Resource::Texture(handle)) =
                (slot.ty, item.resource)
            {
                check_storage_image(slot, &self.store.texture(handle)?.texture, None)?;
            }
        }
        self.check_aliasing(gpu, bound, &[])
    }

    /// A pass cannot use one texture as a storage image it writes and as anything else, so the
    /// textures bound to the program's slots must differ wherever one of the slots writes.
    fn check_aliasing(
        &self,
        gpu: &GpuLayout,
        bound: &[BoundResource],
        imports: &[Import],
    ) -> Result<(), ProgramError> {
        let mut uses: Vec<(&Slot, wgpu::Texture)> = Vec::new();
        for item in bound {
            if let (Some(slot), Resource::Texture(handle)) =
                (gpu.layout.slot(item.set, item.binding), item.resource)
            {
                uses.push((slot, self.store.texture(handle)?.texture.clone()));
            }
        }
        for import in imports {
            let semantic = BindingSource::Engine(import.semantic.clone());
            let slot = gpu.layout.slots().iter().find(|s| s.source == semantic);
            if let (Some(slot), ImportedResource::Texture(texture)) = (slot, &import.resource) {
                uses.push((slot, texture.texture.clone()));
            }
        }
        uses.sort_by_key(|(slot, _)| (slot.set, slot.binding));
        for (index, (first, texture)) in uses.iter().enumerate() {
            let clash = uses[index + 1..]
                .iter()
                .find(|(second, other)| other == texture && (writes(first) || writes(second)));
            if let Some((second, _)) = clash {
                return Err(ProgramError::AliasedTexture {
                    set: first.set,
                    binding: first.binding,
                    other_set: second.set,
                    other_binding: second.binding,
                });
            }
        }
        Ok(())
    }
}

/// A bound texture suits a storage image slot when its format is the one the shader names, it
/// allows storage binding, it has one sample per texel and its shape is the one the slot's view
/// dimension takes: a 2D slot a texture of one layer, an array slot a 2D texture of any layers
/// and a 3D slot a 3D texture. `view` is how a lent texture's view was made, when the owner
/// states it; it must be the slot's dimension.
fn check_storage_image(
    slot: &Slot,
    texture: &wgpu::Texture,
    view: Option<wgpu::TextureViewDimension>,
) -> Result<(), ProgramError> {
    let wgpu::BindingType::StorageTexture {
        format,
        view_dimension,
        ..
    } = slot.ty
    else {
        return Ok(());
    };
    let refuse = |error| ProgramError::StorageImage {
        set: slot.set,
        binding: slot.binding,
        error,
    };
    if texture.format() != format {
        return Err(refuse(StorageImageError::Format {
            declared: format,
            found: texture.format(),
        }));
    }
    if !texture
        .usage()
        .contains(wgpu::TextureUsages::STORAGE_BINDING)
    {
        return Err(refuse(StorageImageError::Usage {
            usage: texture.usage(),
        }));
    }
    if texture.sample_count() > 1 {
        return Err(refuse(StorageImageError::Multisampled {
            samples: texture.sample_count(),
        }));
    }
    let layers = texture.depth_or_array_layers();
    let shape = match (view_dimension, texture.dimension()) {
        (wgpu::TextureViewDimension::D1, wgpu::TextureDimension::D1)
        | (wgpu::TextureViewDimension::D2Array, wgpu::TextureDimension::D2)
        | (wgpu::TextureViewDimension::D3, wgpu::TextureDimension::D3) => true,
        (wgpu::TextureViewDimension::D2, wgpu::TextureDimension::D2) => layers == 1,
        _ => false,
    };
    if !shape || view.is_some_and(|made| made != view_dimension) {
        return Err(refuse(StorageImageError::Dimension {
            declared: view_dimension,
            found: texture.dimension(),
            layers,
        }));
    }
    Ok(())
}
