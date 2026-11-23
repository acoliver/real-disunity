//! Stock declarations lower into the shared raster model without changing their operations. A
//! compute pass lowers into the uses it reads and the storage writes it makes, at its position.
use std::collections::BTreeMap;

use super::raster::{StockCompute, StockPass};
use super::{
    CheckedView, ColorAttachment, ContractError, DepthAttachment, RasterId, RasterPass,
    ResourceLifetime, ResourceRegistry, ResourceUse, Semantic, TextureRange, UseKind,
};
use crate::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeatureDeclaration, PassDeclaration,
    ResourceRef, StageId,
};

/// What the frame holds for each resource: its allocation and the view it attaches.
type Held<'a> = BTreeMap<ResourceRef, (super::AllocationKey, &'a wgpu::TextureView)>;

fn import_views<'a>(
    registry: &mut ResourceRegistry,
    views: &'a [AttachmentView],
) -> Result<Held<'a>, ContractError> {
    views
        .iter()
        .map(|view| {
            registry
                .import(
                    view.texture(),
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    views
                        .iter()
                        .any(|other| {
                            other.texture() == view.texture() && other.allocation_initialized()
                        })
                        .then_some(0),
                    &[],
                )
                .map(|key| (view.resource(), (key, view.view())))
        })
        .collect()
}

pub(crate) fn compile(
    device: &wgpu::Device,
    passes: &[PassDeclaration],
    views: &[AttachmentView],
    features: &[FeatureDeclaration],
    registry: &mut ResourceRegistry,
) -> Result<super::raster::StockRaster, ContractError> {
    let keys = import_views(registry, views)?;
    let mut versions = initial_versions(views);
    let prepared = prepared_writes(registry, &keys, &mut versions, views)?;
    let mut raster = Vec::new();
    for (index, pass) in passes.iter().enumerate() {
        let uses = features
            .iter()
            .flat_map(|feature| &feature.passes)
            .filter(|feature| feature.entry() == pass.entry())
            .flat_map(|feature| &feature.reads)
            .map(|resource| {
                checked_view(registry, &keys, *resource, versions[resource])
                    .map(|view| ResourceUse::read(view, UseKind::Sampled))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let id = RasterId::new(
            StageId::new("stock geometry", 0),
            u32::try_from(index).expect("bounded pass count"),
        );
        if pass.compute.is_some() {
            let writes = storage_writes(registry, &keys, &mut versions, pass)?;
            raster.push(StockPass::Compute(StockCompute {
                id,
                uses: uses.into_iter().chain(writes).collect(),
            }));
            continue;
        }
        let colors = pass
            .color
            .iter()
            .chain(&pass.additional_colors)
            .map(|attachment| {
                let (view, before) = output(registry, &keys, &mut versions, attachment)?;
                Ok(Some(ColorAttachment {
                    view,
                    before,
                    load: attachment.load,
                    store: attachment.store,
                    resolve: None,
                }))
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        let depth = pass
            .depth
            .as_ref()
            .map(|attachment| {
                let (view, before) = depth_view(registry, &keys, &mut versions, attachment)?;
                Ok(DepthAttachment {
                    view,
                    before,
                    load: attachment.load,
                    store: attachment.store,
                    read_only: attachment.read_only,
                })
            })
            .transpose()?;
        raster.push(StockPass::Raster(Box::new(RasterPass {
            id,
            colors,
            depth,
            uses,
            bindings: Vec::new(),
        })));
    }
    registry.validate_stock_after(device, &prepared, &raster)?;
    Ok(super::raster::StockRaster {
        passes: raster,
        prepared,
    })
}

/// The storage writes of compute pass `pass`, each a new version of its subresource, as an
/// attachment's output is.
fn storage_writes(
    registry: &ResourceRegistry,
    keys: &Held<'_>,
    versions: &mut BTreeMap<ResourceRef, u32>,
    pass: &PassDeclaration,
) -> Result<Vec<ResourceUse>, ContractError> {
    pass.storage_writes()
        .iter()
        .map(|write| {
            let version = versions
                .get(&write.resource)
                .map_or(0, |version| version + 1);
            versions.insert(write.resource, version);
            checked_view(registry, keys, write.resource, version)
                .map(|view| ResourceUse::write(view, UseKind::Storage))
        })
        .collect()
}

/// The copy writes a frame's preparation makes before its first pass, one for each view the plan
/// declared prepared, versioned as an attachment's output is.
fn prepared_writes(
    registry: &ResourceRegistry,
    keys: &Held<'_>,
    versions: &mut BTreeMap<ResourceRef, u32>,
    views: &[AttachmentView],
) -> Result<Vec<ResourceUse>, ContractError> {
    views
        .iter()
        .filter(|view| view.prepared_contents())
        .map(|view| {
            let resource = view.resource();
            let version = versions.get(&resource).map_or(0, |version| version + 1);
            versions.insert(resource, version);
            checked_view(registry, keys, resource, version)
                .map(|view| ResourceUse::write(view, UseKind::Copy))
        })
        .collect()
}
fn initial_versions(views: &[AttachmentView]) -> BTreeMap<ResourceRef, u32> {
    // Initialization belongs to the allocation. Newly written sibling layers
    // must advance beyond its initial version even when they clear their contents.
    views
        .iter()
        .filter(|view| {
            views
                .iter()
                .any(|other| other.texture() == view.texture() && other.allocation_initialized())
        })
        .map(|view| (view.resource(), 0))
        .collect()
}

fn checked_view(
    registry: &ResourceRegistry,
    keys: &Held<'_>,
    resource: ResourceRef,
    version: u32,
) -> Result<CheckedView, ContractError> {
    let (key, held) = keys[&resource];
    registry.view_holding(
        key,
        TextureRange::single(resource.mip, resource.layer, wgpu::TextureAspect::All),
        version,
        held,
    )
}
fn output(
    registry: &ResourceRegistry,
    keys: &Held<'_>,
    versions: &mut BTreeMap<ResourceRef, u32>,
    attachment: &AttachmentDeclaration,
) -> Result<(CheckedView, Option<u32>), ContractError> {
    let before = (attachment.load == AttachmentLoad::Load).then(|| versions[&attachment.resource]);
    let version = versions
        .get(&attachment.resource)
        .map_or(0, |version| version + 1);
    versions.insert(attachment.resource, version);
    Ok((
        checked_view(registry, keys, attachment.resource, version)?,
        before,
    ))
}
/// A read-only depth reads the current version and records no write, so the
/// pass's sampled reads and later passes see the same version.
fn depth_view(
    registry: &ResourceRegistry,
    keys: &Held<'_>,
    versions: &mut BTreeMap<ResourceRef, u32>,
    attachment: &AttachmentDeclaration,
) -> Result<(CheckedView, Option<u32>), ContractError> {
    // A read-only clear takes the writing path so validation refuses it as a raster error.
    if !attachment.read_only || attachment.load != AttachmentLoad::Load {
        return output(registry, keys, versions, attachment);
    }
    let version = versions[&attachment.resource];
    Ok((
        checked_view(registry, keys, attachment.resource, version)?,
        Some(version),
    ))
}
