//! Stock declarations lower into the shared raster model without changing their operations.
use std::collections::BTreeMap;

use super::{
    CheckedView, ColorAttachment, CompiledRaster, ContractError, DepthAttachment, RasterId,
    RasterPass, ResourceLifetime, ResourceRegistry, ResourceUse, Semantic, TextureRange, UseKind,
};
use crate::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeatureDeclaration, PassDeclaration,
    ResourceRef, StageId,
};

fn import_views(
    registry: &mut ResourceRegistry,
    views: &[AttachmentView],
) -> Result<BTreeMap<ResourceRef, super::AllocationKey>, ContractError> {
    views
        .iter()
        .map(|view| {
            registry
                .import(
                    view.texture(),
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    None,
                )
                .map(|key| (view.resource(), key))
        })
        .collect()
}

pub(crate) fn compile(
    device: &wgpu::Device,
    passes: &[PassDeclaration],
    views: &[AttachmentView],
    features: &[FeatureDeclaration],
) -> Result<CompiledRaster, ContractError> {
    let mut registry = ResourceRegistry::default();
    let keys = import_views(&mut registry, views)?;
    let mut versions = BTreeMap::<ResourceRef, u32>::new();
    let mut raster = Vec::new();
    for (index, pass) in passes.iter().enumerate() {
        let uses = features
            .iter()
            .flat_map(|feature| &feature.passes)
            .filter(|feature| feature.slot == pass.slot)
            .flat_map(|feature| &feature.reads)
            .map(|resource| {
                checked_view(&registry, &keys, *resource, versions[resource])
                    .map(|view| ResourceUse::read(view, UseKind::Sampled))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let color = pass
            .color
            .as_ref()
            .map(|attachment| {
                let (view, before) = output(&registry, &keys, &mut versions, attachment)?;
                Ok(ColorAttachment {
                    view,
                    before,
                    load: attachment.load,
                    store: attachment.store,
                    resolve: None,
                })
            })
            .transpose()?;
        let depth = pass
            .depth
            .as_ref()
            .map(|attachment| {
                let (view, before) = output(&registry, &keys, &mut versions, attachment)?;
                Ok(DepthAttachment {
                    view,
                    before,
                    load: attachment.load,
                    store: attachment.store,
                    read_only: false,
                })
            })
            .transpose()?;
        raster.push(RasterPass {
            id: RasterId::new(
                StageId::new("stock geometry", 0),
                u32::try_from(index).expect("bounded pass count"),
            ),
            colors: color.map_or_else(Vec::new, |color| vec![Some(color)]),
            depth,
            uses,
            bindings: Vec::new(),
        });
    }
    registry.compile(device, &raster)
}
fn checked_view(
    registry: &ResourceRegistry,
    keys: &BTreeMap<ResourceRef, super::AllocationKey>,
    resource: ResourceRef,
    version: u32,
) -> Result<CheckedView, ContractError> {
    registry.view(
        keys[&resource],
        TextureRange::single(resource.mip, resource.layer, wgpu::TextureAspect::All),
        version,
    )
}
fn output(
    registry: &ResourceRegistry,
    keys: &BTreeMap<ResourceRef, super::AllocationKey>,
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
