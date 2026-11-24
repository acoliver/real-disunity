fn color_attachment(color: &ColorAttachment) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view: &color.view.view,
        depth_slice: None,
        resolve_target: color.resolve.as_ref().map(|view| &view.view),
        ops: wgpu::Operations {
            load: match color.load {
                AttachmentLoad::Load => wgpu::LoadOp::Load,
                AttachmentLoad::ClearColor(value) => wgpu::LoadOp::Clear(value),
                AttachmentLoad::ClearDepth(_) => unreachable!("validated color load"),
            },
            store: color.store,
        },
    }
}
fn depth_attachment(depth: &DepthAttachment) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view: &depth.view.view,
        stencil_ops: stencil_operations(
            depth.view.view.texture().format(),
            depth.view.range().aspect,
            depth.read_only,
            depth.load,
            depth.store,
        ),
        depth_ops: (!depth.read_only).then_some(wgpu::Operations {
            load: match depth.load {
                AttachmentLoad::Load => wgpu::LoadOp::Load,
                AttachmentLoad::ClearDepth(value) => wgpu::LoadOp::Clear(value),
                AttachmentLoad::ClearColor(_) => unreachable!("validated depth load"),
            },
            store: depth.store,
        }),
    }
}

fn stencil_operations(
    format: wgpu::TextureFormat,
    aspect: wgpu::TextureAspect,
    read_only: bool,
    load: AttachmentLoad,
    store: wgpu::StoreOp,
) -> Option<wgpu::Operations<u32>> {
    (!read_only && aspect == wgpu::TextureAspect::All && format.has_stencil_aspect()).then_some(
        wgpu::Operations {
            load: if load == AttachmentLoad::Load {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(0)
            },
            store,
        },
    )
}

use super::{
    AccessMode, CheckedBindings, CheckedView, ContractError, ResourceRegistry, ResourceUse, UseKind,
};
use crate::{AttachmentLoad, StageId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterId {
    pub stage: StageId,
    pub occurrence: u32,
}
impl RasterId {
    pub const fn new(stage: StageId, occurrence: u32) -> Self {
        Self { stage, occurrence }
    }
}
#[derive(Clone, Debug)]
pub struct ColorAttachment {
    pub view: CheckedView,
    /// Required exactly when loading a prior version.
    pub before: Option<u32>,
    pub load: AttachmentLoad,
    pub store: wgpu::StoreOp,
    pub resolve: Option<CheckedView>,
}
#[derive(Clone, Debug)]
pub struct DepthAttachment {
    pub view: CheckedView,
    pub before: Option<u32>,
    pub load: AttachmentLoad,
    pub store: wgpu::StoreOp,
    pub read_only: bool,
}

/// Indexed MRT attachments and a caller-authored position, independent of enum order.
#[derive(Clone, Debug)]
pub struct RasterPass {
    pub id: RasterId,
    pub colors: Vec<Option<ColorAttachment>>,
    pub depth: Option<DepthAttachment>,
    pub uses: Vec<ResourceUse>,
    pub bindings: Vec<CheckedBindings>,
}
impl RasterPass {
    pub(super) fn accesses(&self) -> Vec<ResourceUse> {
        let mut uses = self.uses.clone();
        for color in self.colors.iter().flatten() {
            uses.push(attachment_use(
                &color.view,
                color.before,
                color.load,
                color.store,
            ));
            if let Some(resolve) = &color.resolve {
                uses.push(ResourceUse::write(resolve.clone(), UseKind::Attachment));
            }
        }
        if let Some(depth) = &self.depth {
            let mut usage = attachment_use(&depth.view, depth.before, depth.load, depth.store);
            if depth.read_only {
                usage.access = AccessMode::Read;
            }
            uses.push(usage);
        }
        uses
    }
    fn validate_writes(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ContractError> {
        for usage in self.accesses() {
            usage.view.retain_lease(encoder)?;
        }
        super::capture::validate_writes(
            encoder,
            self.accesses()
                .into_iter()
                .filter(|usage| usage.access != AccessMode::Read)
                .map(|usage| super::capture::write_key(usage.view.allocation())),
        )
    }
    pub(super) fn encode_named(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        label: &str,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let colors: Vec<_> = self
            .colors
            .iter()
            .map(|color| color.as_ref().map(color_attachment))
            .collect();
        let depth = self.depth.as_ref().map(depth_attachment);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            timestamp_writes: stamp.map(crate::timing::Stamp::render),
            color_attachments: &colors,
            depth_stencil_attachment: depth,
            ..Default::default()
        });
        for (index, bindings) in self.bindings.iter().enumerate() {
            pass.set_bind_group(
                u32::try_from(index).expect("validated bind group count"),
                bindings.group(),
                bindings.dynamic_offsets(),
            );
        }
        draw(&mut pass);
    }
}
fn attachment_use(
    view: &CheckedView,
    before: Option<u32>,
    load: AttachmentLoad,
    store: wgpu::StoreOp,
) -> ResourceUse {
    ResourceUse {
        view: view.clone(),
        before,
        access: if load == AttachmentLoad::Load {
            AccessMode::ReadWrite
        } else {
            AccessMode::Write
        },
        kind: UseKind::Attachment,
        store: store == wgpu::StoreOp::Store,
    }
}

/// A validated snapshot. Generation checks are repeated before any encoding.
pub struct CompiledRaster {
    pub(super) passes: Vec<RasterPass>,
    pub(super) labels: Vec<String>,
}
/// One entry of a stock schedule: a raster pass, or a compute pass with no attachments.
pub(crate) enum StockPass {
    Raster(Box<RasterPass>),
    Compute(StockCompute),
}

/// A compute pass of a stock schedule: what it samples and the storage writes it makes.
pub(crate) struct StockCompute {
    pub(super) id: RasterId,
    pub(super) uses: Vec<ResourceUse>,
}
impl StockCompute {
    /// Retains the leases of what the pass uses and offers each write to the capture the frame
    /// belongs to, as a raster pass's are.
    fn validate_writes(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ContractError> {
        for usage in &self.uses {
            usage.view.retain_lease(encoder)?;
        }
        super::capture::validate_writes(
            encoder,
            self.uses
                .iter()
                .filter(|usage| usage.access != AccessMode::Read)
                .map(|usage| super::capture::write_key(usage.view.allocation())),
        )
    }
}
impl StockPass {
    pub(super) const fn id(&self) -> RasterId {
        match self {
            Self::Raster(pass) => pass.id,
            Self::Compute(pass) => pass.id,
        }
    }
}

/// Stock schedules select a geometry identity at encode time, without owned labels.
pub(crate) struct StockRaster {
    pub(super) passes: Vec<StockPass>,
    /// The copy writes the frame's preparation makes before the first pass.
    pub(super) prepared: Vec<ResourceUse>,
}
impl StockRaster {
    /// The version each prepared write makes, in the order of the frame's views.
    #[cfg(test)]
    pub(crate) fn prepared_versions(&self) -> Vec<u32> {
        self.prepared
            .iter()
            .map(|usage| usage.view.version())
            .collect()
    }
    /// Retains the leases of what the frame's preparation writes and offers each write to the
    /// capture the frame belongs to, as a pass's writes are offered before it is encoded.
    pub(crate) fn validate_prepared_writes(
        &self,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<(), ContractError> {
        for usage in &self.prepared {
            usage.view.retain_lease(encoder)?;
        }
        super::capture::validate_writes(
            encoder,
            self.prepared
                .iter()
                .map(|usage| super::capture::write_key(usage.view.allocation())),
        )
    }
    /// The checked view of every color attachment, pass by pass.
    #[cfg(test)]
    pub(crate) fn color_views(&self) -> Vec<&CheckedView> {
        self.raster_passes()
            .flat_map(|pass| pass.colors.iter().flatten().map(|color| &color.view))
            .collect()
    }
    /// The checked view of every sampled read, pass by pass.
    #[cfg(test)]
    pub(crate) fn sampled_views(&self) -> Vec<&CheckedView> {
        self.raster_passes()
            .flat_map(|pass| pass.uses.iter().map(|usage| &usage.view))
            .collect()
    }
    /// The raster passes of the schedule, which are the ones that attach colors and sample.
    #[cfg(test)]
    fn raster_passes(&self) -> impl Iterator<Item = &RasterPass> {
        self.passes.iter().filter_map(|pass| match pass {
            StockPass::Raster(pass) => Some(&**pass),
            StockPass::Compute(_) => None,
        })
    }
    #[cfg(test)]
    pub(crate) fn ids(&self) -> impl Iterator<Item = RasterId> + '_ {
        self.passes.iter().map(StockPass::id)
    }
    /// Each pass's depth (before, attached) versions, for read-only version tests.
    #[cfg(test)]
    pub(crate) fn depth_versions(&self) -> Vec<Option<(Option<u32>, u32)>> {
        self.passes
            .iter()
            .map(|pass| match pass {
                StockPass::Raster(pass) => pass
                    .depth
                    .as_ref()
                    .map(|depth| (depth.before, depth.view.version())),
                StockPass::Compute(_) => None,
            })
            .collect()
    }

    pub(crate) fn encode_stock_pass(
        &self,
        index: usize,
        entry: crate::PassEntry,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) -> Result<(), ContractError> {
        let StockPass::Raster(pass) = &self.passes[index] else {
            unreachable!("{entry} is a compute pass, which the executor records as one");
        };
        pass.validate_writes(encoder)?;
        let label = crate::PassId::Geometry(entry).encoder_label();
        pass.encode_named(encoder, stamp, label.as_ref(), draw);
        Ok(())
    }

    /// Opens compute pass `index` of the schedule, labelled and timed as a geometry pass of
    /// `entry`, after offering its writes to the frame's capture, and lets `record` record into
    /// it.
    ///
    /// # Errors
    /// The capture's refusal of a write it does not own, and `record`'s error.
    pub(crate) fn encode_stock_compute(
        &self,
        index: usize,
        entry: crate::PassEntry,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        record: impl FnOnce(&mut wgpu::ComputePass<'_>) -> Result<(), crate::FrameError>,
    ) -> Result<(), crate::FrameError> {
        let StockPass::Compute(compute) = &self.passes[index] else {
            unreachable!("{entry} is a render pass, which the executor encodes as one");
        };
        compute.validate_writes(encoder)?;
        let label = crate::PassId::Geometry(entry).encoder_label();
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label.as_ref()),
            timestamp_writes: stamp.map(crate::timing::Stamp::compute),
        });
        record(&mut pass)
    }
}
impl CompiledRaster {
    pub fn ids(&self) -> impl Iterator<Item = RasterId> + '_ {
        self.passes.iter().map(|pass| pass.id)
    }
    pub fn encode(
        &self,
        registry: &ResourceRegistry,
        encoder: &mut wgpu::CommandEncoder,
        mut draw: impl FnMut(RasterId, &mut wgpu::RenderPass<'_>),
    ) -> Result<(), ContractError> {
        for pass in &self.passes {
            pass.validate_writes(encoder)?;
            for usage in pass.accesses() {
                registry.check_view(&usage.view)?;
            }
            for bindings in &pass.bindings {
                bindings.validate(registry, pass)?;
            }
        }
        for (pass, label) in self.passes.iter().zip(&self.labels) {
            pass.encode_named(encoder, None, label, |render| draw(pass.id, render));
        }
        Ok(())
    }
}

#[cfg(test)]
mod stencil_tests {
    use super::*;
    #[test]
    fn ownership_is_cleared_and_loaded_with_depth_and_not_used_by_read_only_views() {
        use wgpu::{TextureAspect as A, TextureFormat as F};
        let seed = stencil_operations(
            F::Depth32FloatStencil8,
            A::All,
            false,
            AttachmentLoad::ClearDepth(1.0),
            wgpu::StoreOp::Store,
        )
        .unwrap();
        assert_eq!(seed.load, wgpu::LoadOp::Clear(0));
        assert_eq!(seed.store, wgpu::StoreOp::Store);
        let consume = stencil_operations(
            F::Depth32FloatStencil8,
            A::All,
            false,
            AttachmentLoad::Load,
            wgpu::StoreOp::Store,
        )
        .unwrap();
        assert_eq!(consume.load, wgpu::LoadOp::Load);
        for (format, aspect, read_only) in [
            (F::Depth32Float, A::All, false),
            (F::Depth32FloatStencil8, A::DepthOnly, false),
            (F::Depth32FloatStencil8, A::All, true),
        ] {
            assert!(
                stencil_operations(
                    format,
                    aspect,
                    read_only,
                    AttachmentLoad::Load,
                    wgpu::StoreOp::Store
                )
                .is_none()
            );
        }
    }
}
