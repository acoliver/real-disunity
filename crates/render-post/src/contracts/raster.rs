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
        stencil_ops: None,
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
    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let colors: Vec<_> = self
            .colors
            .iter()
            .map(|color| color.as_ref().map(color_attachment))
            .collect();
        let depth = self.depth.as_ref().map(depth_attachment);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.id.stage.name),
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
}
impl CompiledRaster {
    pub(crate) fn encode_stock_pass(
        &self,
        index: usize,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) -> Result<(), ContractError> {
        self.passes[index].validate_writes(encoder)?;
        self.passes[index].encode(encoder, stamp, draw);
        Ok(())
    }
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
        for pass in &self.passes {
            pass.encode(encoder, None, |render| draw(pass.id, render));
        }
        Ok(())
    }
}
