//! A renderer, caller-authored passes and a feature that counts what the executor asks of it.
use crate::compositor::{FrameRenderer, attachment};
use crate::device::GpuContext;
use std::{cell::Cell, cell::RefCell, rc::Rc};
use worldrunner_render_gpu::submission::PrepareError;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentView, BorrowedFeature, DrawContext, FeatureDeclaration,
    FeaturePass, FrameError, FramePlan, PassDeclaration, PassEntry, PassSlot, PrepareContext,
    PreparedFeature, ResourceRef, ToneMap, color_texture,
};

pub(super) const SIZE: u32 = 16;
pub(super) const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(super) const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Counts its preparations and records the entry of every pass it is asked to draw in.
#[derive(Default)]
pub(super) struct Counting {
    pub prepared: Rc<Cell<u32>>,
    pub draws: Rc<RefCell<Vec<PassEntry>>>,
}

pub(super) struct Feature(pub Counting);

impl PreparedFeature for Feature {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.0.prepared.set(self.0.prepared.get() + 1);
        Ok(())
    }
    fn draw(&self, context: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        self.0.draws.borrow_mut().push(context.entry);
    }
}

/// A feature declaration drawing in each entry, with no formats of its own.
pub(super) fn declaring(entries: &[PassEntry]) -> FeatureDeclaration {
    FeatureDeclaration {
        passes: entries
            .iter()
            .map(|entry| FeaturePass::nth(entry.slot(), entry.occurrence()))
            .collect(),
        effects: Vec::new(),
    }
}

pub(super) struct Rig {
    context: GpuContext,
    renderer: FrameRenderer,
    format: wgpu::TextureFormat,
    textures: Vec<wgpu::Texture>,
}

impl Rig {
    pub(super) fn new() -> Self {
        let context = GpuContext::shared_for_test().unwrap();
        let renderer =
            FrameRenderer::hdr(&context, [SIZE, SIZE], OUTPUT, 1, ToneMap::Reinhard, None).unwrap();
        let format = renderer.targets().scene().texture().format();
        Self {
            context,
            renderer,
            format,
            textures: Vec::new(),
        }
    }

    pub(super) fn device(&self) -> &wgpu::Device {
        self.context.device()
    }

    /// The format of the renderer's scene colour.
    pub(super) fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub(super) fn view(
        &mut self,
        name: &'static str,
        format: wgpu::TextureFormat,
    ) -> AttachmentView {
        let texture = color_texture(self.context.device(), name, [SIZE, SIZE], format);
        let view = AttachmentView::new(ResourceRef::new(name), &texture).unwrap();
        self.textures.push(texture);
        view
    }

    /// A colour target in the scene format that its pass clears.
    pub(super) fn colour(&mut self, name: &'static str) -> (PassDeclaration, AttachmentView) {
        let view = self.view(name, self.format);
        let mut pass = PassDeclaration::new(PassSlot::Opaque);
        pass.color = Some(attachment(view.resource(), &view, false));
        (pass, view)
    }

    /// A depth target in [`DEPTH`] and the attachment that clears it.
    pub(super) fn depth(&mut self, name: &'static str) -> (AttachmentDeclaration, AttachmentView) {
        let view = self.view(name, DEPTH);
        (attachment(view.resource(), &view, true), view)
    }

    /// Encodes `plan` through the frozen entry in one transaction and submits it.
    pub(super) fn encode(
        &mut self,
        plan: (&[PassDeclaration], &[AttachmentView]),
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        let context = &self.context;
        let device = context.device();
        let renderer = &mut self.renderer;
        let transaction = context
            .submission()
            .prepare(device, |encoder, _| {
                renderer.encode_frozen(
                    device,
                    encoder,
                    &FramePlan {
                        passes: plan.0,
                        views: plan.1,
                        requested: &[],
                        operations: &[],
                    },
                    features,
                )
            })
            .map_err(|error| match error {
                PrepareError::Preparation(error) => error,
                other => panic!("the transaction did not start: {other:?}"),
            })?;
        transaction.submit().unwrap();
        Ok(())
    }
}
