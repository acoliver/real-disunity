use std::collections::BTreeMap;

use super::{
    AccessMode, AllocationKey, CheckedView, CompiledRaster, ContractError, RasterPass,
    ResourceLifetime, ResourceRegistry, ResourceUse, UseKind,
};
use crate::AttachmentLoad;

type Cell = (AllocationKey, u32, u32, u8);
#[derive(Default)]
pub(super) struct Versions(BTreeMap<Cell, (Option<u32>, Option<u32>)>);

impl ResourceRegistry {
    /// Checks all declarations before uploads or command encoding. Never sorts or culls.
    pub fn compile(
        &self,
        device: &wgpu::Device,
        passes: &[RasterPass],
    ) -> Result<CompiledRaster, ContractError> {
        let mut versions = Versions::default();
        for (index, pass) in passes.iter().enumerate() {
            if passes[..index].iter().any(|other| other.id == pass.id) {
                return Err(ContractError::Raster(pass.id));
            }
            for binding in &pass.bindings {
                binding.validate_buffers(self, &[])?;
            }
            self.validate_pass(device, pass, &mut versions)
                .map_err(|source| ContractError::Pass {
                    pass: pass.id,
                    source: Box::new(source),
                })?;
        }
        Ok(CompiledRaster {
            passes: passes.to_vec(),
        })
    }
    pub(super) fn validate_pass(
        &self,
        device: &wgpu::Device,
        pass: &RasterPass,
        versions: &mut Versions,
    ) -> Result<(), ContractError> {
        validate_attachments(self, device, pass)?;
        for usage in &pass.uses {
            if matches!(usage.kind, UseKind::Copy | UseKind::Attachment) {
                return Err(ContractError::Binding(usage.view.allocation));
            }
        }
        for binding in &pass.bindings {
            binding.validate(self, pass)?;
        }
        let accesses = pass.accesses();
        for (index, usage) in accesses.iter().enumerate() {
            self.check_view(&usage.view)?;
            validate_usage(self, usage)?;
            for previous in &accesses[..index] {
                if previous.view.allocation == usage.view.allocation
                    && previous.view.range.overlaps(&usage.view.range)
                {
                    validate_overlap(device, previous, usage)?;
                }
            }
        }
        for usage in &accesses {
            validate_versions(self, usage, versions)?;
        }
        Ok(())
    }
}

pub(super) fn validate_overlap(
    device: &wgpu::Device,
    previous: &ResourceUse,
    usage: &ResourceUse,
) -> Result<(), ContractError> {
    if previous.access != AccessMode::Read || usage.access != AccessMode::Read {
        return Err(ContractError::Hazard(usage.view.allocation));
    }
    // GLES lacks read-only depth/stencil attachment transitions. The other
    // supported rendering backends expose this capability unconditionally.
    if device.adapter_info().backend == wgpu::Backend::Gl
        && (previous.kind == UseKind::Attachment || usage.kind == UseKind::Attachment)
        && previous.kind != usage.kind
    {
        return Err(ContractError::DepthSampling(usage.view.allocation));
    }
    Ok(())
}

pub(super) fn validate_usage(
    registry: &ResourceRegistry,
    usage: &ResourceUse,
) -> Result<(), ContractError> {
    let allocation = registry.allocation(usage.view.allocation)?;
    let writes = usage.access != AccessMode::Read;
    if writes && allocation.lifetime == ResourceLifetime::ImportedReadOnly {
        return Err(ContractError::ReadOnly(allocation.key));
    }
    let required = match (usage.kind, usage.access) {
        (UseKind::Attachment, _) => wgpu::TextureUsages::RENDER_ATTACHMENT,
        (UseKind::Sampled, AccessMode::Read) => wgpu::TextureUsages::TEXTURE_BINDING,
        (UseKind::Storage, _) => wgpu::TextureUsages::STORAGE_BINDING,
        (UseKind::Copy, AccessMode::Read) => wgpu::TextureUsages::COPY_SRC,
        (UseKind::Copy, AccessMode::Write | AccessMode::ReadWrite) => wgpu::TextureUsages::COPY_DST,
        _ => return Err(ContractError::Binding(allocation.key)),
    };
    if !allocation.texture.usage().contains(required)
        || (usage.access == AccessMode::Write && usage.before.is_some())
        || (usage.access != AccessMode::Write && usage.before.is_none())
    {
        return Err(ContractError::Binding(allocation.key));
    }
    Ok(())
}

pub(super) fn validate_versions(
    registry: &ResourceRegistry,
    usage: &ResourceUse,
    versions: &mut Versions,
) -> Result<(), ContractError> {
    let allocation = registry.allocation(usage.view.allocation)?;
    let aspects = aspect_cells(allocation.texture.format(), usage.view.range.aspect);
    for mip in usage.view.range.mips.clone() {
        for layer in usage.view.range.layers.clone() {
            for &aspect in &aspects {
                let cell = versions
                    .0
                    .entry((allocation.key, mip, layer, aspect))
                    .or_insert((allocation.initial, allocation.initial));
                if usage.access != AccessMode::Write && (cell.0.is_none() || cell.0 != usage.before)
                {
                    return Err(ContractError::Producer(allocation.key));
                }
                if usage.access == AccessMode::Read {
                    if usage.before != Some(usage.view.version) {
                        return Err(ContractError::Producer(allocation.key));
                    }
                } else {
                    if cell.1.is_some_and(|version| usage.view.version <= version) {
                        return Err(ContractError::Producer(allocation.key));
                    }
                    cell.0 = usage.store.then_some(usage.view.version);
                    cell.1 = Some(usage.view.version);
                }
            }
        }
    }
    Ok(())
}
fn aspect_cells(format: wgpu::TextureFormat, aspect: wgpu::TextureAspect) -> Vec<u8> {
    match aspect {
        wgpu::TextureAspect::DepthOnly => vec![1],
        wgpu::TextureAspect::StencilOnly => vec![2],
        _ if format.has_depth_aspect() && format.has_stencil_aspect() => vec![1, 2],
        _ if format.has_depth_aspect() => vec![1],
        _ if format.has_stencil_aspect() => vec![2],
        _ => vec![0],
    }
}

fn validate_attachments(
    registry: &ResourceRegistry,
    device: &wgpu::Device,
    pass: &RasterPass,
) -> Result<(), ContractError> {
    if (pass.colors.iter().all(Option::is_none) && pass.depth.is_none())
        || pass.colors.len() > device.limits().max_color_attachments as usize
        || pass.bindings.len() > device.limits().max_bind_groups as usize
    {
        return Err(ContractError::Raster(pass.id));
    }
    let mut shape = None;
    let mut total_bytes: u32 = 0;
    for color in pass.colors.iter().flatten() {
        let actual = attachment_shape(registry, device, &color.view, false)?;
        let format = registry.allocation(color.view.allocation)?.texture.format();
        let alignment = format
            .target_component_alignment()
            .expect("validated color format");
        total_bytes = total_bytes.next_multiple_of(alignment)
            + format
                .target_pixel_byte_cost()
                .expect("validated color format");
        if shape.is_some_and(|expected| expected != actual) || !valid_color_load(color.load) {
            return Err(ContractError::Raster(pass.id));
        }
        shape = Some(actual);
        validate_load(color.load, color.before, pass)?;
        if let Some(resolve) = &color.resolve {
            validate_resolve(registry, device, &color.view, resolve, pass)?;
        }
    }
    let limit = device.limits().max_color_attachment_bytes_per_sample;
    if total_bytes > limit {
        return Err(ContractError::ColorAttachmentBytes {
            total: total_bytes,
            limit,
        });
    }
    if let Some(depth) = &pass.depth {
        let actual = attachment_shape(registry, device, &depth.view, true)?;
        if shape.is_some_and(|expected| expected != actual)
            || !matches!(
                depth.load,
                AttachmentLoad::Load | AttachmentLoad::ClearDepth(_)
            )
            || matches!(depth.load, AttachmentLoad::ClearDepth(value) if !value.is_finite() || !(0.0..=1.0).contains(&value))
            || (depth.read_only
                && (depth.load != AttachmentLoad::Load || depth.store != wgpu::StoreOp::Store))
        {
            return Err(ContractError::Raster(pass.id));
        }
        validate_load(depth.load, depth.before, pass)?;
    }
    Ok(())
}
fn valid_color_load(load: AttachmentLoad) -> bool {
    match load {
        AttachmentLoad::Load => true,
        AttachmentLoad::ClearColor(color) => [color.r, color.g, color.b, color.a]
            .iter()
            .all(|value| value.is_finite()),
        AttachmentLoad::ClearDepth(_) => false,
    }
}
fn validate_load(
    load: AttachmentLoad,
    before: Option<u32>,
    pass: &RasterPass,
) -> Result<(), ContractError> {
    if (load == AttachmentLoad::Load) != before.is_some() {
        return Err(ContractError::Raster(pass.id));
    }
    Ok(())
}
fn attachment_shape(
    registry: &ResourceRegistry,
    device: &wgpu::Device,
    view: &CheckedView,
    depth: bool,
) -> Result<([u32; 2], u32), ContractError> {
    registry.check_view(view)?;
    let texture = &registry.allocation(view.allocation)?.texture;
    let features = texture
        .format()
        .guaranteed_format_features(device.features());
    if view.range.mips.len() != 1
        || view.range.layers.len() != 1
        || texture.format().is_depth_stencil_format() != depth
        || texture.format().has_stencil_aspect()
        || (depth
            && (!texture.format().has_depth_aspect()
                || view.range.aspect == wgpu::TextureAspect::StencilOnly))
        || !features
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        || !features
            .flags
            .sample_count_supported(texture.sample_count())
    {
        return Err(ContractError::Binding(view.allocation));
    }
    Ok((
        [
            (texture.width() >> view.range.mips.start).max(1),
            (texture.height() >> view.range.mips.start).max(1),
        ],
        texture.sample_count(),
    ))
}
fn validate_resolve(
    registry: &ResourceRegistry,
    device: &wgpu::Device,
    source: &CheckedView,
    target: &CheckedView,
    pass: &RasterPass,
) -> Result<(), ContractError> {
    let (source_size, samples) = attachment_shape(registry, device, source, false)?;
    let (target_size, target_samples) = attachment_shape(registry, device, target, false)?;
    let source_allocation = registry.allocation(source.allocation)?;
    let target_allocation = registry.allocation(target.allocation)?;
    if samples == 1
        || target_samples != 1
        || source_size != target_size
        || source_allocation.texture.format() != target_allocation.texture.format()
        || source_allocation.semantic != target_allocation.semantic
        || !source_allocation
            .texture
            .format()
            .guaranteed_format_features(device.features())
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
    {
        return Err(ContractError::Raster(pass.id));
    }
    Ok(())
}
