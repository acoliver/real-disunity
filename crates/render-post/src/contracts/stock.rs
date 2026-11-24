//! Stock declarations lower into the shared raster model without changing their operations. A
//! compute pass lowers into the uses it reads and the storage writes it makes, at its position.
use super::raster::{StockCompute, StockPass};
use super::{
    AllocationKey, CheckedView, ColorAttachment, ContractError, DepthAttachment, RasterId,
    RasterPass, ResourceLifetime, ResourceRegistry, ResourceUse, Semantic, TextureRange, UseKind,
};
use crate::plan_hash::{PlanMap, map_with_capacity};
use crate::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeatureDeclaration, PassDeclaration,
    ResourceRef, StageId,
    frame::{EntryFeatures, Textures},
};

/// What the frame holds of one resource: its allocation, the view it attaches, and the version
/// the frame has written it to so far, if it has contents.
struct Held<'a> {
    key: AllocationKey,
    view: &'a wgpu::TextureView,
    version: Option<u32>,
}
impl Held<'_> {
    /// The version the frame's contents are at. A validated frame reads and loads only what it or
    /// an earlier submission has written.
    fn current(&self) -> u32 {
        self.version.expect("a read or load of written contents")
    }

    /// The version of the next write: the first for a resource without contents.
    fn next(&mut self) -> u32 {
        let version = self.version.map_or(0, |version| version + 1);
        self.version = Some(version);
        version
    }

    fn checked_view(
        &self,
        registry: &ResourceRegistry,
        resource: ResourceRef,
        version: u32,
    ) -> Result<CheckedView, ContractError> {
        registry.view_holding(
            self.key,
            TextureRange::single(resource.mip, resource.layer, wgpu::TextureAspect::All),
            version,
            self.view,
        )
    }
}

/// What the frame holds for each resource.
struct Frame<'a>(PlanMap<ResourceRef, Held<'a>>);
impl<'a> Frame<'a> {
    /// Imports each view's texture once, at version zero when any view of it declares initialized
    /// contents, and holds the view for its resource.
    fn import(
        registry: &mut ResourceRegistry,
        views: &'a [AttachmentView],
    ) -> Result<Self, ContractError> {
        let mut textures = Textures::default();
        let numbers: Vec<usize> = views
            .iter()
            .map(|view| textures.number(view.texture()))
            .collect();
        let mut initialized = vec![false; textures.len()];
        for (view, &number) in views.iter().zip(&numbers) {
            initialized[number] |= view.allocation_initialized();
        }
        let mut keys = vec![None; textures.len()];
        let mut held = map_with_capacity(views.len());
        for (view, &number) in views.iter().zip(&numbers) {
            let initial = initialized[number].then_some(0);
            let key = if let Some(key) = keys[number] {
                key
            } else {
                let key = registry.import(
                    view.texture(),
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    initial,
                    &[],
                )?;
                keys[number] = Some(key);
                key
            };
            held.insert(
                view.resource(),
                Held {
                    key,
                    view: view.view(),
                    version: initial,
                },
            );
        }
        Ok(Self(held))
    }

    fn held(&mut self, resource: ResourceRef) -> &mut Held<'a> {
        self.0
            .get_mut(&resource)
            .expect("a resource the frame holds")
    }

    /// The copy writes a frame's preparation makes before its first pass, one for each view the
    /// plan declared prepared, versioned as an attachment's output is.
    fn prepared_writes(
        &mut self,
        registry: &ResourceRegistry,
        views: &[AttachmentView],
    ) -> Result<Vec<ResourceUse>, ContractError> {
        views
            .iter()
            .filter(|view| view.prepared_contents())
            .map(|view| {
                let resource = view.resource();
                let held = self.held(resource);
                let version = held.next();
                held.checked_view(registry, resource, version)
                    .map(|view| ResourceUse::write(view, UseKind::Copy))
            })
            .collect()
    }

    /// The storage writes of compute pass `pass`, each a new version of its subresource, as an
    /// attachment's output is.
    fn storage_writes(
        &mut self,
        registry: &ResourceRegistry,
        pass: &PassDeclaration,
        uses: &mut Vec<ResourceUse>,
    ) -> Result<(), ContractError> {
        for write in pass.storage_writes() {
            let held = self.held(write.resource);
            let version = held.next();
            let view = held.checked_view(registry, write.resource, version)?;
            uses.push(ResourceUse::write(view, UseKind::Storage));
        }
        Ok(())
    }

    fn output(
        &mut self,
        registry: &ResourceRegistry,
        attachment: &AttachmentDeclaration,
    ) -> Result<(CheckedView, Option<u32>), ContractError> {
        let held = self.held(attachment.resource);
        let before = (attachment.load == AttachmentLoad::Load).then(|| held.current());
        let version = held.next();
        Ok((
            held.checked_view(registry, attachment.resource, version)?,
            before,
        ))
    }

    /// A read-only depth reads the current version and records no write, so the
    /// pass's sampled reads and later passes see the same version.
    fn depth_view(
        &mut self,
        registry: &ResourceRegistry,
        attachment: &AttachmentDeclaration,
    ) -> Result<(CheckedView, Option<u32>), ContractError> {
        // A read-only clear takes the writing path so validation refuses it as a raster error.
        if !attachment.read_only || attachment.load != AttachmentLoad::Load {
            return self.output(registry, attachment);
        }
        let held = self.held(attachment.resource);
        let version = held.current();
        Ok((
            held.checked_view(registry, attachment.resource, version)?,
            Some(version),
        ))
    }
}

pub(crate) fn compile(
    device: &wgpu::Device,
    passes: &[PassDeclaration],
    views: &[AttachmentView],
    features: &[FeatureDeclaration],
    registry: &mut ResourceRegistry,
) -> Result<super::raster::StockRaster, ContractError> {
    let mut frame = Frame::import(registry, views)?;
    let prepared = frame.prepared_writes(registry, views)?;
    let entries = EntryFeatures::new(passes, features);
    let mut raster = Vec::with_capacity(passes.len());
    for (index, pass) in passes.iter().enumerate() {
        let mut uses = Vec::new();
        for resource in entries.of_pass(index).flat_map(|feature| &feature.reads) {
            let held = frame.held(*resource);
            let view = held.checked_view(registry, *resource, held.current())?;
            uses.push(ResourceUse::read(view, UseKind::Sampled));
        }
        let id = RasterId::new(
            StageId::new("stock geometry", 0),
            u32::try_from(index).expect("bounded pass count"),
        );
        if pass.compute.is_some() {
            frame.storage_writes(registry, pass, &mut uses)?;
            raster.push(StockPass::Compute(StockCompute { id, uses }));
            continue;
        }
        let mut colors = Vec::with_capacity(1 + pass.additional_colors.len());
        for attachment in pass.color.iter().chain(&pass.additional_colors) {
            let (view, before) = frame.output(registry, attachment)?;
            colors.push(Some(ColorAttachment {
                view,
                before,
                load: attachment.load,
                store: attachment.store,
                resolve: None,
            }));
        }
        let depth = pass
            .depth
            .as_ref()
            .map(|attachment| {
                let (view, before) = frame.depth_view(registry, attachment)?;
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
