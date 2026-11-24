use std::{cell::Cell, rc::Rc};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
};
use worldrunner_render_scene::wgpu;

#[derive(Clone, Default)]
pub(super) struct Refusal {
    armed: Rc<Cell<bool>>,
    preparations: Rc<Cell<u32>>,
}

impl Refusal {
    pub(super) fn set_refusing(&self, armed: bool) {
        self.armed.set(armed);
    }

    pub(super) fn preparations(&self) -> u32 {
        self.preparations.get()
    }

    pub(super) fn error() -> FrameError {
        FrameError::FeaturePreparation {
            feature: "public stock retry fixture",
            reason: "caller refused suffix preparation".into(),
        }
    }
}

impl FrameFeature for Refusal {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration::new(PassSlot::Opaque)
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        _: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let entry = PassEntry::new(PassSlot::Opaque);
        let opaque = passes
            .iter()
            .find(|pass| pass.entry() == entry)
            .ok_or(FrameError::AttachmentContract(entry))?;
        let mut declaration = self.declare();
        let pass = &mut declaration.passes[0];
        pass.color_format = opaque.color.as_ref().map(|color| color.format);
        pass.depth_format = opaque.depth.as_ref().map(|depth| depth.format);
        pass.additional_color_formats = opaque
            .additional_colors
            .iter()
            .map(|color| color.format)
            .collect();
        Ok(declaration)
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Refusal {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.preparations.set(self.preparations.get() + 1);
        if self.armed.get() {
            return Err(Self::error());
        }
        Ok(())
    }

    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}
